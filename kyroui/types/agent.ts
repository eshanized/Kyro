export interface Conversation {
  id: string;
  title: string;
  model: string;
  created_at: number;
}

/** Origin of the active agent profile content. */
export type ProfileOrigin = "builtin" | "override" | "custom";

/**
 * One agent profile in the registry. The Markdown manifest is authoritative;
 * this record is a read-model the roster UI renders and edits.
 */
export interface AgentProfileRecord {
  id: string;
  name: string;
  avatar: string;
  role: string;
  description: string;
  model_id?: string | null;
  origin: ProfileOrigin;
  enabled: boolean;
  runtime_disabled: boolean;
  schema_version: number;
  version_label: string;
  aliases: string[];
  validation_error?: string | null;
  source_path?: string | null;
  instructions: string;
  tools_mode: string;
  tools_allow: string[];
  delegation_enabled: boolean;
  delegation_max_depth: number;
  delegation_max_children: number;
  max_steps: number;
  is_builtin: boolean;
}

/** Raw Markdown manifest of an agent profile for the editor. */
export interface AgentProfileDocument {
  id: string;
  document: string;
  source_path?: string | null;
}

export interface CreateAgentProfileRequest {
  id: string;
  name: string;
  avatar: string;
  role: string;
  description: string;
  instructions: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: any;
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: any;
}

export interface ToolResult {
  tool_call_id: string;
  name: string;
  output: string;
  is_error: boolean;
}

export interface AgentStepEvent {
  /** The run that produced the event. */
  run_id: string;
  /** The conversation the run belongs to, when it is known. */
  conversation_id?: string | null;
  /** The agent that produced the event, when it is known. */
  agent_id?: string | null;
  /** The run this event was delegated from, for child-agent output. */
  parent_run_id?: string | null;
  /** True when the event was produced by a delegated child run. */
  is_child_run?: boolean;
  step: number;
  thought: string;
  tool_calls: ToolCall[];
  tool_results: ToolResult[];
  is_finished: boolean;
}

export interface Message {
  id: string;
  conversation_id: string;
  role: "user" | "assistant" | "tool" | "system";
  content: string;
  tool_calls_json?: string;
  observations_json?: string;
  created_at: number;
}

export interface ApiKeys {
  nvidia?: string;
  telegram_bot_token?: string;
  telegram_chat_id?: string;
}

export interface ApiKeysMetadata {
  nvidia_configured: boolean;
  telegram_configured: boolean;
  telegram_chat_id?: string;
}

export interface AgentRun {
  id: string;
  conversation_id?: string;
  agent_id: string;
  model: string;
  prompt: string;
  status: "queued" | "running" | "waiting_approval" | "succeeded" | "failed" | "cancelled" | "interrupted";
  created_at: number;
  started_at?: number;
  completed_at?: number;
  error?: string;
  config_json?: string;
  parent_run_id?: string;
}

export interface RunEvent {
  id: string;
  run_id: string;
  seq: number;
  event_type: string;
  payload_json: string;
  created_at: number;
}

export interface ApprovalRequest {
  id: string;
  run_id: string;
  tool_name: string;
  summary: string;
  args_json: string;
  risk_level: "safe" | "moderate" | "destructive";
  reason: string;
  status: "pending" | "approved" | "denied" | "expired";
  created_at: number;
  expires_at: number;
  resolved_at?: number;
  resolved_by?: string;
}

export interface MemoryEntry {
  id: string;
  scope: string;
  owner_id: string;
  key: string;
  value: string;
  source: string;
  created_at: number;
  updated_at: number;
}

export interface SandboxStatus {
  available: boolean;
  engine: "docker" | "podman" | "none";
  message: string;
}

export interface WindowInfo {
  id: number;
  title: string;
  app_name: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SkillStep {
  action_type: string;
  x?: number;
  y?: number;
  text?: string;
  key?: string;
  delay_ms: number;
}

export interface SkillRecord {
  id: string;
  name: string;
  description: string;
  steps_json: string;
  created_at: number;
}

export interface RoutineRecord {
  id: string;
  name: string;
  cron_expression: string;
  action_type: string;
  target_id: string;
  enabled: boolean;
  last_run?: number;
  next_run?: number;
}

export interface BotRecord {
  id: string;
  name: string;
  avatar: string;
  role: string;
  system_prompt: string;
  is_default: boolean;
}

export interface BotStatus {
  id: string;
  name: string;
  avatar: string;
  role: string;
  state: "idle" | "busy" | "delegating";
  current_task?: string;
  /** Every active objective, keyed by the run that owns it. */
  active_tasks?: { run_id: string; objective: string }[];
}

export interface TakeoverStatus {
  is_active: boolean;
  reason: string;
  timestamp: number;
}

export interface GovernancePolicy {
  shell_mode: string; // "SafeAutoApprove" | "SandboxOnly" | "AskAlways"
  computer_use_mode: string; // "FullAutonomous" | "ConfirmDestructive" | "AskAlways"
  require_takeover_for_credentials: boolean;
  blocked_domains: string[];
}

export interface McpServerRecord {
  id: string;
  name: string;
  command: string;
  args_json: string;
  env_json: string;
  enabled: boolean;
}

export interface GhostStatus {
  is_enabled: boolean;
  display: string;
  resolution: string;
}

export interface GroundedMark {
  id: number;
  x: number;
  y: number;
  /** The bounding box of the detected element. */
  width: number;
  height: number;
  label: string;
  /** The detected role, or "candidate" when the role could not be established. */
  role: string;
  /** Confidence in the detection (0..1). */
  confidence: number;
  /** The target identity: a semantic element or a visual candidate. */
  target: { kind: string; value: any };
}

export interface GroundedScreenResult {
  width: number;
  height: number;
  base64_png: string;
  marks: GroundedMark[];
  /** How the marks were produced ("visual_candidate_detection" or "browser_semantic"). */
  source: string;
  /** The fingerprint of the analysed frame; marks are bound to it. */
  fingerprint: string;
  /** True when no candidate cleared the confidence threshold. */
  low_confidence: boolean;
}

export interface TimelineStepRecord {
  id: string;
  run_id?: string;
  step: number;
  action_name: string;
  click_x?: number;
  click_y?: number;
  before_base64: string;
  after_base64: string;
  diff_percent: number;
  timestamp: number;
}

export type TimelineStep = TimelineStepRecord;

export interface InstalledApp {
  name: string;
  exec: string;
  comment: string;
  categories: string[];
}

export interface ArtifactRecord {
  id: string;
  run_id?: string;
  conversation_id?: string;
  name: string;
  file_path: string;
  artifact_type: string; // "code" | "markdown" | "image" | "json" | "log" | "document"
  mime_type: string;
  size_bytes: number;
  created_at: number;
  metadata_json?: string;
}

export interface SearchResultItem {
  category: string; // "conversation" | "message" | "memory" | "skill" | "routine" | "artifact"
  id: string;
  title: string;
  snippet: string;
  score: number;
  timestamp: number;
}

export interface TemplatePack {
  schema_version: number;
  name: string;
  description: string;
  created_at: number;
  bots: BotRecord[];
  skills: SkillRecord[];
  routines: RoutineRecord[];
  mcp_servers: McpServerRecord[];
}

export interface ImportSummary {
  bots_imported: number;
  skills_imported: number;
  routines_imported: number;
  mcp_servers_imported: number;
}

export interface SystemHealthInfo {
  db_connected: boolean;
  db_wal_mode: boolean;
  docker_available: boolean;
  docker_message: string;
  ghost_xvfb_available: boolean;
  os_keyring_available: boolean;
  display_server: string;
  tools: Record<string, boolean>;
}

export interface ConnectorCatalogItem {
  id: string;
  name: string;
  connector_type: "native" | "remote_mcp" | "local_mcp";
  description: string;
  icon: string;
  auth_type: string;
  capabilities: string[];
  required_scopes: string[];
  is_builtin: boolean;
  is_available: boolean;
  doc_url?: string;
  connected_accounts_count: number;
  status: "connected" | "available" | "needs_auth" | "needs_reauth" | "disabled" | "error" | "not_configured" | "catalog_only";
}

export interface ConnectedAccountSummary {
  id: string;
  connector_id: string;
  account_identifier: string;
  display_name: string;
  status: "connected" | "connecting" | "needs_auth" | "needs_reauth" | "disconnected" | "error" | "disabled";
  status_message?: string;
  granted_scopes: string[];
  config: Record<string, any>;
  created_at: number;
  updated_at: number;
  last_connected_at?: number;
  last_error?: string;
  tool_count: number;
}

export interface OAuthFlowInit {
  session_id: string;
  auth_url: string;
  redirect_uri: string;
  expires_at: number;
}

export interface ConnectorToolPolicyRecord {
  id: string;
  account_id: string;
  tool_name: string;
  policy: "allow" | "ask" | "deny";
  risk_level: "safe" | "moderate" | "destructive";
  updated_at: number;
  schema_fingerprint?: string;
}

export interface ConnectorToolDefinition {
  name: string;
  description: string;
  parameters: any;
  risk_level: "safe" | "moderate" | "destructive";
  is_read_only: boolean;
}

export interface OAuthSessionStatusView {
  session_id: string;
  provider_id: string;
  status:
    | "pending"
    | "callback_received"
    | "exchanging"
    | "finalizing"
    | "completed"
    | "failed"
    | "cancelled"
    | "expired";
  error?: string;
  created_at: number;
  expires_at: number;
}

export interface AccountToolView {
  name: string;
  original_name: string;
  description: string;
  is_read_only: boolean;
  risk_level: "safe" | "moderate" | "destructive";
  default_policy: "allow" | "ask";
  can_auto_approve?: boolean;
  schema_fingerprint?: string;
}

export interface BrowserInfo {
  id: string;
  name: string;
  display_name: string;
  family: string;
  path: string;
  version?: string | null;
  available: boolean;
  automatable: boolean;
  adapter: string;
  unavailable_reason?: string | null;
  platform?: string | null;
}

export interface BrowserCapabilities {
  browsers: BrowserInfo[];
  default_browser_id?: string | null;
  driver: {
    name: string;
    version: string;
    status: string;
    bidi_supported: boolean;
    cdp_supported?: boolean;
    external_driver_required: boolean;
  };
  node_version?: string | null;
  platform?: string | null;
}

export interface BrowserSettings {
  selected_browser: string;
  headless_default: boolean;
}

export interface ModelCapabilities {
  provider: string;
  model_id: string;
  text_input: boolean;
  text_output: boolean;
  vision: boolean;
  image_input?: boolean;
  tool_calling: boolean;
  supported_formats: string[];
  max_image_size_bytes?: number | null;
  max_tokens?: number | null;
  capability_status: string;
  configuration_error?: string | null;
}

export interface OrganizationRule {
  name: string;
  custom_extensions: Record<string, string>;
  group_duplicates: boolean;
  duplicates_folder_name: string;
  recursive: boolean;
}

export interface PlannedFileOp {
  source_path: string;
  destination_path: string;
  category: string;
  reason: string;
  file_size: number;
  file_hash?: string | null;
  is_duplicate: boolean;
  duplicate_of?: string | null;
  conflict?: string | null;
}

export interface OrganizationPlan {
  plan_id: string;
  target_directory: string;
  total_scanned: number;
  duplicates_count: number;
  conflicts_count: number;
  operations: PlannedFileOp[];
}

export interface OrganizationOpRecord {
  source_path: string;
  destination_path: string;
  file_hash: string;
  file_size: number;
  status: "completed" | "failed" | "undone";
  error?: string | null;
}

export interface OrganizationTransaction {
  transaction_id: string;
  target_directory: string;
  created_at: number;
  status: "completed" | "partially_failed" | "undone";
  operations: OrganizationOpRecord[];
}

export interface ActionItem {
  text: string;
  priority: "high" | "medium" | "normal";
  line_number?: number | null;
}

export interface ExtractedDate {
  raw_date: string;
  context: string;
}

export interface DocumentSection {
  heading: string;
  content: string;
}

export interface DocumentMetadata {
  file_name: string;
  file_path: string;
  format: string;
  size_bytes: number;
  page_count?: number | null;
  word_count: number;
  character_count: number;
  is_local_only: boolean;
}

export interface DocumentAnalysisResult {
  metadata: DocumentMetadata;
  summary: string;
  key_facts: string[];
  action_items: ActionItem[];
  extracted_dates: ExtractedDate[];
  missing_or_ambiguous: string[];
  sections: DocumentSection[];
  privacy_disclosure: string;
  raw_text: string;
}

export interface DocumentComparisonResult {
  doc_a_path: string;
  doc_b_path: string;
  summary_differences: string;
  common_topics: string[];
  added_in_b: string[];
  removed_from_a: string[];
  action_items_comparison: string[];
  markdown_report: string;
}

export interface VisualDefect {
  defect_type: string;
  description: string;
  location?: string | null;
  severity: string;
}

export interface VisualRegion {
  name: string;
  bounds?: string | null;
  dom_ref?: string | null;
  description: string;
}

export interface VisualDiscrepancy {
  dom_reference?: string | null;
  description: string;
}

export interface VisualAnalysisResult {
  summary: string;
  key_regions: VisualRegion[];
  visible_defects: VisualDefect[];
  discrepancies: VisualDiscrepancy[];
  confidence: string;
  supporting_evidence: string;
  uncertainty: string;
  analyzed_by_model?: string | null;
  artifact_id?: string | null;
  vision_supported: boolean;
}

export interface FileDiff {
  file_path: string;
  original_content?: string | null;
  new_content: string;
  additions: number;
  deletions: number;
  unified_diff: string;
  risk_assessment: "low" | "medium" | "high";
}

export interface CodeCheckpoint {
  checkpoint_id: string;
  workspace_path: string;
  description: string;
  created_at: number;
  pre_existing_dirty_files: string[];
  modified_files: FileDiff[];
  status: "active" | "rolled_back";
}

export interface RollbackResult {
  checkpoint_id: string;
  files_reverted: string[];
  conflicts_detected: string[];
  status: "success" | "partial" | "failed";
}

export interface BuildDoctorDiagnostic {
  root_cause_error: string;
  file?: string | null;
  line_number?: number | null;
  error_code?: string | null;
  category: "syntax" | "type_error" | "missing_import" | "test_failure" | "dependency" | "runtime";
  suggested_repair: string;
  reproduced: boolean;
}

export interface VisualBaseline {
  project_path: string;
  route: string;
  viewport: string;
  base64_png: string;
  created_at: number;
  artifact_id?: string | null;
}

export interface VisualRegressionResult {
  has_regression: boolean;
  diff_percent: number;
  route: string;
  viewport: string;
  description: string;
  baseline_artifact?: string | null;
  current_artifact?: string | null;
}

export interface SpecialistTask {
  task_id: string;
  /** The agent profile that runs this stage (resolved via the registry). */
  agent_id: string;
  objective: string;
  depends_on: string[];
  optional: boolean;
  isolated_worktree: boolean;
  /** Retry budget: retries + 1 attempts total. */
  max_attempts: number;
  /** Attempts consumed so far. */
  attempts: number;
  status: "pending" | "running" | "completed" | "failed" | "skipped" | "cancelled";
  child_run_id?: string | null;
  isolated_worktree_path?: string | null;
  worktree_branch?: string | null;
  output?: string | null;
  error?: string | null;
  started_at?: number | null;
  completed_at?: number | null;
}

export interface WorkflowTaskSpec {
  task_id: string;
  agent_id: string;
  /** `{{objective}}` is replaced by the workspace's primary objective. */
  objective: string;
  depends_on: string[];
  optional: boolean;
  isolated_worktree: boolean;
  /** Retry count after the first attempt (0 = single attempt). */
  retries: number;
}

export interface WorkflowSpec {
  id: string;
  name: string;
  description: string;
  /** 1..=8 concurrent task batches. */
  concurrency: number;
  on_failure: "abort" | "continue";
  tasks: WorkflowTaskSpec[];
}

export interface MultiAgentWorkspace {
  workspace_id: string;
  parent_run_id: string;
  project_path: string;
  workflow_id: string;
  workflow_name: string;
  primary_objective: string;
  concurrency: number;
  on_failure: "abort" | "continue";
  tasks: SpecialistTask[];
  status: "active" | "completed" | "failed" | "failed_partial" | "cancelled" | "reconciled";
  created_at: number;
  updated_at: number;
}

export interface ReconciliationOutcome {
  workspace_id: string;
  changes_detected: boolean;
  modified_files: string[];
  unified_diff: string;
  conflicts_detected: string[];
  can_safely_merge: boolean;
  reconciliation_summary: string;
}

export interface RoutineParameter {
  name: string;
  label: string;
  default_value: string;
  is_secret: boolean;
  description: string;
}

export interface RoutineStep {
  id: string;
  title: string;
  action_type: "tool_call" | "prompt" | "skill";
  tool_name?: string | null;
  parameters: any;
  expected_output?: string | null;
  timeout_ms: number;
  on_error: "stop" | "retry" | "continue";
  requires_approval: boolean;
}

export interface ProjectCandidate {
  name: string;
  path: string;
  kind: string;
  manifest?: string | null;
  lockfile?: string | null;
  package_manager?: string | null;
  is_git_repo: boolean;
  scripts: string[];
  name_matched: boolean;
}

export interface DiscoveryResult {
  searched_root: string;
  candidates: ProjectCandidate[];
  exact_match?: ProjectCandidate | null;
  error?: string | null;
}

export interface DevServerState {
  id: string;
  pid: number;
  working_directory: string;
  command: string;
  status: "starting" | "ready" | "failed" | "stopped";
  failure?: string | null;
  error_message?: string | null;
  exit_code?: number | null;
  ready_url?: string | null;
  port?: number | null;
  framework?: string | null;
  package_manager?: string | null;
  started_at: number;
  ready_at?: number | null;
  run_id?: string | null;
}

export interface EditableRoutine {
  version: "kyro.routine.v1";
  id: string;
  name: string;
  description: string;
  cron_expression?: string | null;
  enabled: boolean;
  parameters: RoutineParameter[];
  steps: RoutineStep[];
  created_at: number;
  updated_at: number;
  trusted: boolean;
}

export interface AuthorizedFolderRecord {
  id: string;
  path: string;
  display_name: string;
  added_at: number;
}

export interface RoutineRunRecord {
  id: string;
  routine_id: string;
  routine_name: string;
  status: "running" | "succeeded" | "failed" | "cancelled";
  started_at: number;
  completed_at?: number | null;
  summary?: string | null;
  error?: string | null;
}

export interface DocumentSearchResult {
  path: string;
  file_name: string;
  format: string;
  size_bytes: number;
  modified_at: number;
  matched_by: string;
  snippet?: string | null;
  relevance_score: number;
}

export interface DuplicateFileItem {
  path: string;
  size_bytes: number;
  modified_at: number;
  is_original: boolean;
}

export interface DuplicateGroup {
  file_hash: string;
  size_bytes: number;
  files: DuplicateFileItem[];
  evidence: string;
}

export interface SafeFileOpResult {
  operation: string;
  source_path: string;
  destination_path?: string | null;
  success: boolean;
  requires_confirmation: boolean;
  message: string;
  undoable: boolean;
}

export interface RevisionFlashcard {
  question: string;
  answer: string;
}

export interface StudyNotesResult {
  doc_name: string;
  doc_path: string;
  format: string;
  overview: string;
  core_concepts: string[];
  key_dates_and_milestones: string[];
  summary_points: string[];
  revision_flashcards: RevisionFlashcard[];
}

export interface TodayEventItem {
  id: string;
  title: string;
  time_display: string;
  location?: string | null;
  html_link?: string | null;
}

export interface TodayEmailSummary {
  unread_count: number;
  recent_subjects: string[];
  is_connected: boolean;
  account_email?: string | null;
  error?: string | null;
}

export interface TodayDocumentItem {
  file_name: string;
  path: string;
  modified_ago: string;
  format: string;
}

export interface TodayRoutineItem {
  id: string;
  name: string;
  schedule: string;
  last_status?: string | null;
  next_run_display?: string | null;
}

export interface TodayServiceStatus {
  service_name: string;
  is_connected: boolean;
  status_text: string;
  can_connect: boolean;
}

export interface SuggestedFolder {
  id: string;
  name: string;
  path: string;
  exists: boolean;
}

export interface PendingApprovalItem {
  id: string;
  run_id: string;
  tool_name: string;
  summary: string;
  risk_level: string;
  reason: string;
  created_at: number;
}

export interface TodayDashboardData {
  greeting: string;
  date_display: string;
  upcoming_events: TodayEventItem[];
  email_summary: TodayEmailSummary;
  recent_documents: TodayDocumentItem[];
  upcoming_routines: TodayRoutineItem[];
  recent_routine_results: RoutineRunRecord[];
  pending_tasks_count: number;
  pending_approvals?: PendingApprovalItem[];
  pending_approvals_count?: number;
  services: TodayServiceStatus[];
  calendar_error?: string | null;
  email_error?: string | null;
}

export interface NlRoutineSuggestion {
  name: string;
  cron_expression: string;
  action_type: string;
  target_id: string;
  explanation: string;
  required_resources: string[];
  requires_confirmation: boolean;
  needs_clarification?: boolean;
  clarification_prompt?: string;
  schedule_confidence?: string;
}

export interface HabitSettingsRecord {
  key: string;
  enabled: boolean;
  min_occurrences: number;
  updated_at: number;
}

export interface HabitSuggestionRecord {
  id: string;
  name: string;
  description: string;
  signature_hash: string;
  occurrences: number;
  confidence: number;
  proposed_action_type: string;
  proposed_payload_json: string;
  status: "pending" | "accepted" | "dismissed" | "snoozed";
  evidence_json: string;
  first_seen_at: number;
  last_seen_at: number;
  updated_at: number;
}

export interface DocumentChunkRecord {
  id: string;
  document_path: string;
  chunk_index: number;
  heading?: string | null;
  start_line?: number | null;
  end_line?: number | null;
  content: string;
  content_hash: string;
  format: string;
  chunk_version: number;
  indexed_at: number;
  embedding_json?: string | null;
}

export interface RetrievalResult {
  chunk_id: string;
  document_path: string;
  heading?: string | null;
  start_line?: number | null;
  end_line?: number | null;
  content: string;
  score: number;
  lexical_score: number;
  semantic_score: number;
  match_type: "lexical" | "semantic" | "hybrid";
}

export interface RagStatus {
  total_documents_indexed: number;
  total_chunks_indexed: number;
  embedding_mode: string;
  embedding_model?: string | null;
  embedding_endpoint?: string | null;
  authorized_folder_count: number;
}

export interface RagSettingsRecord {
  key: string;
  embedding_mode: string;
  embedding_endpoint?: string | null;
  embedding_model?: string | null;
  max_candidates: number;
  updated_at: number;
}

export interface ResourceLimits {
  cpu: number;
  memory_mb: number;
  pids_limit: number;
  timeout_seconds: number;
  max_output_bytes: number;
}

export interface ExperimentSpec {
  id?: string | null;
  name: string;
  runtime: string;
  custom_image?: string | null;
  script?: string | null;
  argv?: string[] | null;
  workspace_path?: string | null;
  network_mode?: string | null;
  limits?: ResourceLimits | null;
}

export interface CodeExperimentRecord {
  id: string;
  name: string;
  run_id?: string | null;
  runtime: string;
  image: string;
  script?: string | null;
  argv_json: string;
  workspace_path?: string | null;
  output_dir?: string | null;
  network_mode: string;
  cpu_limit: number;
  memory_limit_mb: number;
  timeout_seconds: number;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled" | "timed_out" | "interrupted";
  exit_code?: number | null;
  stdout_tail?: string | null;
  stderr_tail?: string | null;
  result_summary?: string | null;
  artifact_ids_json?: string | null;
  created_at: number;
  started_at?: number | null;
  completed_at?: number | null;
  error?: string | null;
}

export interface ExperimentComparisonResult {
  exp_a_id: string;
  exp_a_name: string;
  exp_a_status: string;
  exp_a_exit_code?: number | null;
  exp_b_id: string;
  exp_b_name: string;
  exp_b_status: string;
  exp_b_exit_code?: number | null;
  summary_diff: string;
  stdout_match: boolean;
  exit_codes_match: boolean;
}


