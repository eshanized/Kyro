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
- [x] Add comprehensive unit and integration tests covering security, workspace confinement, shell injection, credential vault, approval CAS consumption, pixel diffs, MCP schemas, timezones, and memory ranking.
- [x] Verify zero warnings under `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] Verify complete frontend build with `npx tsc --noEmit && npm run build`.

### Phase 14: Comprehensive Test Expansion (≥95% Coverage)
- [x] Expand frontend test coverage to &ge;95% with 31 Vitest test suites and 259 passing tests covering components, modals, HUD, settings, and workflows.
- [x] Expand backend Rust test suite to 251 passing tests across unit, integration, browser automation, connector, and security suites.
- [x] Resolve clippy linter warnings across all test targets (`-D warnings`).
- [x] Exclude test files from production `tsc` emit in `tsconfig.json` to ensure clean release builds.

### Phase 15: v0.1.1 Release & Documentation Refactoring
- [x] Bump version to `0.1.1` across `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/Cargo.lock`, `libkyro/tauri.conf.json`, `install.sh`, connectors, and templates.
- [x] Refactor repository documentation (`README.md`, `runtime-overview.md`, `security-boundaries.md`, `connector-system.md`, `feature-matrix.md`, `implementation-log.md`).
- [x] Multi-distribution release pipeline validation and GitHub release publishing.

### Phase 16: v0.1.2 Release — Critical Agent Runtime, Security, Worktree Isolation & Registry Remediation
- [x] Enforce profile-level tool permissions at the actual execution boundary (`execute_tool_governed` in `loop_engine.rs`) with canonical tool name normalization (`canonical_tool_name`) and forged delegation rejection.
- [x] Propagate authoritative isolated worktrees to shell commands (`cmd_builder.current_dir(root)`) and filesystem tools, failing closed immediately if an isolated worktree was requested but does not exist on disk.
- [x] Implement collision-resistant worktree paths and branch naming (`{ws}_{task}_a{attempt}`), base-commit tracking, merge-base detection, and concurrent modification conflict detection in `MultiAgentOrchestrator::reconcile_changes`.
- [x] Implement transactional profile saves with prospective uniqueness validation across candidate entries prior to atomic disk persistence, and preserve `override_error` diagnostics for corrupt override files.
- [x] Prevent multi-agent delegation recursion cycles and self-delegation by walking the SQLite database ancestor chain, enforcing fatal child run persistence, and respecting `model.mode` (`fixed` vs `inherit`).
- [x] Implement context-stripped mention parsing (`strip_non_routable_text`) ignoring code fences, blockquotes, inline backticks, and email addresses, with diagnostic fallback to `apex-orchestrator`.
- [x] Filter context facts strictly by profile `memory_scopes` (`build_context_block_scoped`) to prevent fact leakage.
- [x] Reconcile frontend teammate roster origin badges ("Built-in", "Override", "Custom"), auto-fallback to Apex on active agent deletion/disablement, and preserve form drafts on validation failure.
- [x] Expand automated test coverage to 313 Rust tests and 268 Vitest tests across 33 test files with zero compiler or linter warnings (`cargo clippy --all-targets -- -D warnings`).
- [x] Bump version to `0.1.2` across `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/tauri.conf.json`, `install.sh`, connectors, issue templates, and docs.

### Phase 17: v0.1.3 Release — Modern Shell Redesign, Live Reasoning Streaming, Hardened File Organizer & Multi-Provider Isolation
- [x] Redesign modern desktop application shell (`App.tsx`) with high-density sidebar navigation, welcome experience, quick-prompt suggestion chips, and per-conversation state isolation.
- [x] Implement multi-line auto-expanding `TaskComposer` with drag-and-drop attachment previews, keyboard shortcuts, and `@mention` agent selector.
- [x] Build modular accessible UI component system in `kyroui/components/ui/` (`Button`, `Dialog`, `CodePanel`, `StatusBadge`, `Surface`, `TextField`).
- [x] Implement live model reasoning streaming in `ModelRouter` and build `ThinkingIndicator` component with dark-theme wave/shimmer animations, auto-scrolling reasoning disclosures, and 12px Fira Code typography.
- [x] Harden safe reversible file organizer with server-authoritative anti-tampering via SQLite `file_organization_plans`, pre-move SHA-256 freshness verification, symlink exclusion, authorized folder confinement, durable transactions, and one-click undo recovery (`undo_file_transaction`).
- [x] Integrate native Anthropic Messages API support with system prompt handling, streaming event handling, and sub-route isolation.
- [x] Enforce Compare-And-Swap (CAS) guards on agent run status updates in SQLite, making terminal outcomes immutable.
- [x] Enforce task grant provenance tracking, negation handling, and local policy enforcement on dynamic external connector tools.
- [x] Enforce strict UTF-8 validation and binary magic byte detection on text attachments.
- [x] Expand automated test coverage to 317 Rust tests and 318 Vitest tests across 39 test files with zero compiler or linter warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- [x] Bump version to `0.1.3` across `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/Cargo.lock`, `libkyro/tauri.conf.json`, `install.sh`, connectors, issue templates, and docs.

### Phase 18: v0.1.4 Release — Authoritative Verification Contracts, Truthful Execution Lifecycle, Deep Credential Redaction & Specialized Multi-Agent Personas
- [x] Eliminate untrusted verification metadata acceptance and name-based verification heuristics in `kyroui/components/TaskProgress.tsx`.
- [x] Enforce strict context binding for verification records (`run_id`, `tool_call_id`, `action_name`, expected postcondition, and target resource).
- [x] Enforce action-level outcome tracking in `libkyro/src/storage/db.rs` with strict precedence (`failed` > `uncertain` > `unverified` > `succeeded`) and add `"unverified"` to `VALID_RUN_STATUSES`.
- [x] Enforce truthful assistant messaging (`formatTruthfulAssistantReply()`) across all execution flows when outcomes are unverified.
- [x] Implement deep credential scrubbing (`redactSensitiveString`) and keystroke masking for sensitive computer-use targets.
- [x] Add specialized agent personas (`code-reviewer`, `doc-reviewer`, `test-engineer`) and safe caller API key fallback for child delegations.
- [x] Reconcile multi-agent worktree via terminal DAG candidate resolution, explicit apply actions, and conflict-safe shadow-copying (#27, #28, #29).
- [x] Enhance file organization tool with fine-grained pattern exclusions and descriptive labels.
- [x] Add autonomous workspace architecture walkthrough animation and demo video with Git-LFS tracking.
- [x] Expand automated test coverage to over 320 Rust tests and 365 Vitest tests across 43 test files with zero compiler or linter warnings.
- [x] Bump version to `0.1.4` across `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/Cargo.lock`, `libkyro/tauri.conf.json`, `install.sh`, connectors, issue templates, and docs.
