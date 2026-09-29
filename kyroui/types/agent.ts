export interface Conversation {
  id: string;
  title: string;
  model: string;
  created_at: number;
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
  label: string;
}

export interface GroundedScreenResult {
  width: number;
  height: number;
  base64_png: string;
  marks: GroundedMark[];
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
