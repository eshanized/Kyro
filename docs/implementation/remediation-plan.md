# Kyro Remediation & Implementation Plan

## Executive Roadmap

This document records the implemented remediation phases that evolved the Kyro desktop agent MVP into a production-grade, secure, multi-agent runtime.

---

### Phase 1: Centralized Security and Governance
- [x] Audit and remove hardcoded `autoApprove: true` from frontend and background workers (`kyroui/App.tsx`, `kyroui/hud/HudPanel.tsx`).
- [x] Create `ExecutionRequest` domain model and centralized `AuthorizationEngine` (`libkyro/src/governance/auth.rs`).
- [x] Implement robust shell command parser (handling multiline input, pipelines, shell operators, wrappers like `sh`/`bash`/`sudo`/`env`, env assignments) to replace naive substring checks.
- [x] Enforce `GovernancePolicy` settings (`shell_mode`, `computer_use_mode`, `blocked_domains`, `require_takeover_for_credentials`).
- [x] Fail-closed sandbox enforcement: error when sandbox is unavailable under `SandboxOnly` policy.
- [x] Build durable, race-safe `ApprovalRequest` queue with background expiration and atomic CAS one-time execution semantics (`libkyro/src/storage/db.rs`).
- [x] Restrict filesystem tools to validated canonical workspace paths (`validate_path_confinement`); prevent path traversal (`..`), null bytes, unauthorized absolute paths, and symlink escapes outside workspace root.
- [x] Move security enforcement into privileged Rust execution boundary for direct computer actions (`perform_computer_action` evaluates `evaluate_computer_action`).
- [x] Eliminate tool-name heuristics (e.g. `contains("get")` or `contains("list")`) for MCP tool permissions; require explicit policy or approval.

### Phase 2: Secrets and Storage Integrity
- [x] Replace plaintext `secret:*` SQLite storage with secure OS credential store (`keyring` crate) with authenticated AES-256-GCM encrypted database fallback (`libkyro/src/storage/vault.rs`).
- [x] Replace machine-identifying key derivation with secure key generation (`KYRO_VAULT_KEY` / OS Keyring / `0600` keyfile at `~/.config/kyro/.vault_master.key`).
- [x] Fix keyring health check: replaced unconditional success with a live probe test (write, read, verify, delete).
- [x] Implement zero-secret leakage: return only secret-presence metadata (`ApiKeysMetadata`) to frontend.
- [x] Implement safe database migration: migrate existing plaintext secrets, verify success, and purge plaintext keys idempotently.
- [x] Add versioned, transactional SQLite migration system with PRAGMA foreign keys, WAL mode, and busy timeouts.

### Phase 3: Durable Agent-Run Lifecycle & Approvals
- [x] Define first-class `AgentRun` entity with states: `queued`, `running`, `waiting_approval`, `succeeded`, `failed`, `cancelled`, `interrupted`.
- [x] Fix approval waiting loop to transition run status to `waiting_approval`, expire overdue requests, and unblock on expiration or cancellation without hanging.
- [x] Prevent double execution of approved requests via atomic CAS `consume_approval_request`.
- [x] Persist run inputs, configuration snapshots, timestamps, tool calls, and structured error data.
- [x] Implement startup reconciliation (`reconcile_active_runs_on_startup`): mark orphaned runs as interrupted; provide safe recovery.
- [x] Build persisted event timeline (`run_events`) with monotonic per-run sequence numbers.

### Phase 4: Truthful Cancellation & Subprocess Handling
- [x] Replace global atomic abort flag with per-run `CancellationToken`s with `child_token` hierarchy (`libkyro/src/agent/cancellation.rs`).
- [x] Propagate cancellation to model HTTP completions, tool execution, computer actions, skills, routines, and child agents.
- [x] Implement bounded subprocess execution: registers PIDs, creates dedicated Unix process groups (`process_group(0)`), and kills process group (`libc::kill(-(pid as i32), libc::SIGKILL)`) and `kill_on_drop(true)` on cancellation.
- [x] Ensure truthful cancellation reporting in `cancel_agent_run`: checks run existence, prevents overwriting terminal runs (`succeeded`/`failed`/`completed`), and synchronizes DB status.

### Phase 5: Multimodal Attachments & Visual Verification
- [x] Preserve binary/base64 attachment payloads and MIME types end-to-end from frontend file input to model router as `MessageContent::Parts`.
- [x] Explicitly fail fast when attempting to pass image attachments or visual verification screenshots to non-vision models.
- [x] Compute genuine pixel diff percentages using `TimelineEngine::compute_diff_percent` for `click_mark` and `computer_action` verification steps.
- [x] Ground element coordinates with Set-of-Marks and perform post-action state verification (`libkyro/src/grounding/som.rs`).

### Phase 6: Skill Recording & Governance Replay
- [x] Filter sensitive input (passwords, auth tokens) from recorded skill steps (`libkyro/src/teach_task/recorder.rs`).
- [x] Route each replayed skill step through `AuthorizationEngine::evaluate_computer_action` against governance policy.
- [x] Connect self-healing coordinate recalibration and parameter substitution.

### Phase 7: Multi-Agent Roster & Bot Identity
- [x] Pass `botId` from frontend `run_agent_turn` to ensure agent turns are recorded with the correct agent identity.
- [x] Generate unique child run IDs (`{parent}_child_{bot}_{uuid}`) for delegated sub-tasks with bounded recursion depth (<= 3).
- [x] Track active bot assignments with RAII drop guard to ensure roster status returns to idle on completion or error.

### Phase 8: Server-Scoped MCP & Schema Validation
- [x] Scope MCP tool identities with server prefixes (`mcp__{server_id}__{tool}`) to prevent tool name collision.
- [x] Validate arguments strictly against declared JSON `inputSchema` before dispatch (`validate_schema`).
- [x] Route all MCP tool executions through centralized authorization engine before invocation.

### Phase 9: Timezone-Aware Scheduler
- [x] Parse and evaluate cron expressions with IANA timezones (`chrono-tz`) supporting `CRON_TZ=...` and `TZ=...` directives.
- [x] Prevent overlapping routine executions with active execution set tracking.
- [x] Enforce missed-run policies (skips backlog if elapsed time > 24 hours).

### Phase 10: Fail-Closed Telegram Bridge
- [x] Fail closed when `allowed_chat_id` is empty or does not match incoming chat ID.
- [x] Scrub tokens and credentials from all Telegram logging and error messages.
- [x] Dispatch authorized task requests directly to `AgentLoopEngine::run_step` and report completion/errors back to chat.
- [x] Support remote mobile approval resolution (`/approve <id>`, `/deny <id>`).

### Phase 11: Scope-Aware Relevance-Ranked Memory
- [x] Isolate memory scopes strictly (`preference`, `agent`, `project`, `fact`) to prevent cross-bot leakage.
- [x] Implement deterministic offline relevance scoring based on keyword overlap and recency boost.
- [x] Strictly enforce context budget (`MAX_MEMORY_CHARS = 2500`) with deterministic truncation.

### Phase 12 & 13: Automated Regression Testing
- [x] Add comprehensive unit and integration tests covering security, workspace confinement, shell injection, credential vault, approval CAS consumption, pixel diffs, MCP schemas, timezones, and memory ranking (31 passing tests in `libkyro/tests/engine_tests.rs`).
- [x] Verify zero warnings under `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] Verify complete frontend build with `npx tsc --noEmit && npm run build`.
