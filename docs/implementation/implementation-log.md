# Kyro Implementation Log

This log chronicles the engineering remediation, feature completion, and verification work executed across the Kyro platform codebase.

---

## Session Log

### Phase 1: Baseline Audit & Health Verification
- Audited repository layout (`libkyro/`, `kyroui/`, `docs/`, `package.json`, `Cargo.toml`).
- Executed initial build baseline:
  - TypeScript compiler: `npx tsc --noEmit` (Passed)
  - Vite build: `npm run build` (Passed)
  - Rust tests: `cargo test` (21 tests passed)
  - Rust linter: `cargo clippy --workspace --all-targets --all-features` (Passed)
  - Code formatter: `cargo fmt --all -- --check` (Passed)
- Identified functional gaps according to the Master Remediation Specification:
  1. Artifacts management registry, preview, and export was missing.
  2. Universal search across SQLite tables was missing.
  3. Command palette (`Ctrl+K`) and keyboard shortcuts were missing.
  4. Voice dictation (STT) and text-to-speech (TTS) were missing.
  5. Portable workspace templates and import/export with secret sanitization were missing.
  6. System health diagnostics probe command was missing.
  7. File and image attachment in prompt input was missing.

### Phase 2: Storage & Migration Layer Implementation
- Implemented **Migration 4** in `libkyro/src/storage/db.rs`:
  - Created `artifacts` table with columns: `id`, `run_id`, `conversation_id`, `name`, `file_path`, `artifact_type`, `mime_type`, `size_bytes`, `created_at`, `metadata_json`.
  - Added indexes on `idx_artifacts_run` and `idx_artifacts_conv`.
- Added `ArtifactRecord` and `SearchResultItem` structs in `db.rs` and re-exported in `storage/mod.rs`.
- Added database methods:
  - `create_artifact(&self, artifact: &ArtifactRecord) -> Result<()>`
  - `list_artifacts(&self, run_id: Option<&str>, conversation_id: Option<&str>) -> Result<Vec<ArtifactRecord>>`
  - `get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>>`
  - `delete_artifact(&self, id: &str) -> Result<bool>`
- Implemented **Universal Search** in `Database::search_all`:
  - Queries conversations, messages, scoped memories, skills, routines, and artifacts concurrently.
  - Sorts results by recency (`std::cmp::Reverse(a.timestamp)`).

### Phase 3: Templates & Extensibility Engine
- Created `libkyro/src/storage/templates.rs`:
  - Defined `TemplatePack` and `ImportSummary`.
  - Implemented `TemplateEngine::export_pack` with **strict secret redaction**: any MCP environment variables matching `KEY`, `TOKEN`, `SECRET`, `PASS`, or `AUTH` are converted to `<REDACTED_ON_EXPORT>`.
  - Implemented `TemplateEngine::import_pack` with schema version 1 validation and identifier sanitization (`..`, `/`, `\` rejection).
  - Provided built-in starter templates for DevOps/Systems Engineering and Web Intelligence/QA.

### Phase 4: Centralized Authorization & Tool Loop Enhancement
- Added `create_artifact` tool to `AgentLoopEngine::standard_tools` in `libkyro/src/agent/loop_engine.rs`.
- Integrated `create_artifact` authorization check in `AuthorizationEngine::evaluate` (`libkyro/src/governance/auth.rs`) enforcing traversal protection.
- Enhanced `write_file` tool in `loop_engine.rs` to automatically register newly written files as artifacts in the SQLite registry.
- Enforced strict artifact path confinement to `<app_data_dir>/artifacts/`.

### Phase 5: Tauri IPC Command Expansion
- Added the following Tauri commands in `libkyro/src/commands.rs` and registered in `libkyro/src/main.rs`:
  - `list_artifacts`
  - `get_artifact`
  - `create_artifact`
  - `delete_artifact`
  - `read_artifact_content`
  - `export_artifact`
  - `search_workspace`
  - `export_workspace_templates`
  - `import_workspace_templates`
  - `get_starter_templates`
  - `get_system_health` (probes DB WAL, Docker sandbox, Xvfb Ghost display, OS keyring, and CLI tools `git`, `docker`, `xdotool`, `xvfb`, `xclip`, `python3`, `node`)

### Phase 6: Frontend UI Components & Capabilities
- **Artifacts Registry Modal** (`kyroui/components/ArtifactsModal.tsx`):
  - Category filters (all, code, markdown, json, document), name search, metadata display.
  - Text and code preview window with syntax whitespace preservation.
  - Export to destination file path and artifact deletion.
- **Global Command Palette** (`kyroui/components/CommandPalette.tsx`):
  - Accessible via `Ctrl+K` or `Cmd+K` and top navbar button.
  - Instant universal search querying `search_workspace` with category badges.
  - Quick action list for navigation, toggling Ghost Mode, opening PTY, and switching active bots (@Apex, @Forge, @Scout).
  - Keyboard navigation (Arrow Up/Down, Enter, Escape).
- **Settings & Diagnostics Extension** (`kyroui/components/SettingsModal.tsx`):
  - Added "Templates" tab: 1-click starter pack installation, export workspace pack, import JSON.
  - Added "Diagnostics" tab: live status grid of SQLite WAL, display server, Docker engine, Ghost Xvfb virtual display, and CLI tools.
- **Voice Dictation & Text-to-Speech** (`kyroui/App.tsx`):
  - Voice dictation via standard Web Speech API (delegating transcription to underlying OS/webview speech service) with active microphone indicator.
  - Local speech synthesis audio playback for assistant responses with speak/stop controls.
- **File Attachments in Prompt** (`kyroui/App.tsx`):
  - Paperclip file picker supporting text and image files.
  - Injects formatted file content into task prompt.
- **Desktop Notifications** (`kyroui/App.tsx`):
  - Dispatched on run completion, failure, and approval events via Web Notification API.

### Phase 7: Comprehensive Testing & Verification
- Added 3 new end-to-end integration tests to `libkyro/tests/engine_tests.rs`:
  1. `test_artifacts_lifecycle_and_path_security`: Verifies artifact record insertion, query, deletion, and strict rejection of path traversal (`../`) and slashes.
  2. `test_universal_search`: Verifies cross-table query matching across conversations, messages, and skills.
  3. `test_workspace_templates_export_and_import`: Verifies secret redaction on export, starter packs, and safe schema import into a fresh database.
- Ran test suite: **24 unit and integration tests passed**.
- Ran linter: `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed with 0 warnings.
- Ran formatter: `cargo fmt --all -- --check` passed.
- Ran frontend checks: `npx tsc --noEmit && npm run build` passed with 0 errors.

---

### Phase 8: Generic Browser Automation & Autonomous UI Audit Remediation
- **Authoritative Browser Capability Registry (`libkyro/src/browser/mod.rs`, `worker.mjs`)**:
  - Implemented multi-browser discovery across PATH, standard installation directories, and desktop entry registries.
  - Recognizes Firefox, Firefox ESR, Google Chrome, Chromium, Microsoft Edge, Brave, Vivaldi, and Opera.
  - Detects OS default browser via `xdg-settings` / `xdg-mime` (Linux) and platform-specific equivalents.
  - Probes browser executable versions, user data isolation, and automation adapter compatibility without launching everyday profiles or exposing credentials.
- **Provider-Neutral Worker Engine (`libkyro/browser-worker/worker.mjs`)**:
  - Refactored worker into a unified architecture supporting two native protocol adapters using Node 20+ built-ins (`WebSocket`, `fetch`):
    - `FirefoxSession`: Native WebDriver BiDi over WebSocket.
    - `ChromiumSession`: Chrome DevTools Protocol (CDP) over WebSocket (`Page`, `Runtime`, `Network`, `Log`, `Input`, `Emulation.setDeviceMetricsOverride`).
  - Implemented session creation, semantic page snapshots, accessible element interaction, form filling, mouse/keyboard events, screenshots, console error capture, failed request tracking, and responsive viewport emulation across both engines.
- **Strict Non-Substitution & Selection Hierarchy**:
  - Selection order: 1) Explicit task request, 2) Current audit selection, 3) User saved preference in Kyro DB, 4) OS default when set to Automatic.
  - Rejection with explanation: If the requested browser is unsupported or unavailable, Kyro fails closed, explains why, and lists available alternatives without silently substituting Firefox or another browser.
- **Dynamic Project Discovery & Resolution (`libkyro/src/project/discovery.rs`, `workflow.rs`)**:
  - Removed all hardcoded production defaults for `Runstead` and current directory (`.`).
  - Added dynamic Desktop directory resolution via `dirs::desktop_dir()` with fallback to `$HOME/Desktop`.
  - Added support for explicit project paths (`project_path: Option<PathBuf>`) bypassing discovery when provided.
  - Implemented candidate matching: filters candidates strictly to directories matching query name or package.json `name`; prioritizes exact matches over approximate matches; halts and lists candidates when multiple ambiguous matches exist.
  - If project name or path is omitted, honestly rejects without guessing.
- **Frontend Settings UI (`kyroui/components/SettingsModal.tsx`, `types/agent.ts`)**:
  - Added dedicated "Browser Automation" tab in `SettingsModal`.
  - Displays Automatic (system default) choice with detected browser indicator.
  - Lists all discovered automatable browsers with display name, version, protocol, path, and readiness status.
  - Lists unavailable browsers with structured reason.
  - Exposes Headless Automation Mode toggle (defaulting to headless, visible on request).
  - Provides "Refresh Browser Discovery" and "Save Browser Preferences" actions.
- **Verification & Regression Testing**:
  - `libkyro` unit tests: 115 tests passed.
  - `browser_tests.rs`: 14 integration tests passed (covering Firefox BiDi, Chromium CDP, Brave CDP, navigation, snapshots, semantic clicks, form fills, console errors, screenshots, session isolation, and non-substitution validation).
  - `ui_audit_tests.rs`: 2 end-to-end audit tests passed (running autonomous UI audit against `defect-app` on host Brave/Chromium CDP engine; detecting console errors, 404s, broken images, broken form validations, unhandled controls, mobile horizontal scroll; capturing screenshots and generating report).
  - `kyroui` vitest suite: 7 test files passed (52 tests total, including `browser_settings.test.tsx`).
  - `cargo clippy`: 0 warnings.
  - `npm run build`: built clean.

## 2026-10-22: Everyday Assistant Functional Completion & Security Hardening Remediation

### 1. P0 — Email Header Injection Elimination (`libkyro/src/connector/native/google.rs`)
- **CRLF & Header Injection Rejection**: `GoogleAdapter::encode_rfc2822_message` strictly scans `to`, `subject`, and `in_reply_to` headers for `\r`, `\n`, `\0`, or control characters, returning explicit `Err` on any header injection attempt.
- **Robust Recipient Parsing & Validation**: Implemented `validate_and_sanitize_recipient` and `validate_email_address`. Accurately parses RFC 5322 address formats (`user@example.com` or `Display Name <user@example.com>`), verifies single `@`, non-empty local part, valid domain syntax, and rejects invalid characters or multiple `@` characters.
- **RFC 2047 Word Encoding**: Encodes non-ASCII subjects and display names containing Unicode or special characters into standards-compliant RFC 2047 encoded words (`=?UTF-8?B?...?=`) using base64.
- **Threading Header Sanitation**: Sanitizes `in_reply_to` parameter into strict message-id `<...>` without delimiter escaping, injecting both `In-Reply-To` and `References`.
- **Connector Call Sites & Token Extraction**: Updated `gmail_create_draft` and `gmail_send_email` to handle `Result` from message construction. Implemented `GoogleAdapter::extract_token` to seamlessly unpack JSON secrets stored in vault (`{"access_token": ...}`) or raw bearer tokens for both `execute_tool` and `test_connection`.

### 2. P0 — Provider-Specific OAuth Token Refresh (`libkyro/src/connector/manager.rs`)
- **Google OAuth Refresh Support**: Added token endpoint `https://oauth2.googleapis.com/token` for connector IDs `google` and `google_calendar`. Added provider-specific client ID and secret resolution from account config, vault, and `KYRO_GOOGLE_CLIENT_ID` / `KYRO_GOOGLE_CLIENT_SECRET`.
- **Single-Flight Synchronization**: Lock re-check in `refresh_account_token` checks if token was updated within 15 seconds or if `expires_at > now + 60_000`, immediately reusing refreshed tokens across concurrent calls.
- **Lifecycle Status Transitions**: Correctly classifies permanent authentication errors (`invalid_grant`, `invalid_client`, HTTP 4xx) to transition account to canonical status `"needs_reauth"`. Transient errors (HTTP 5xx, timeouts, connection refused, DNS errors) transition status to `"error"` with detailed error logs. Updated post-401 failure in `dispatch_action` to mark `"needs_reauth"` instead of uncanonical `"reauthorization_required"`.

### 3. P1 — Calendar Availability & Scheduling Semantics (`libkyro/src/connector/native/google.rs`)
- **Cancelled & Transparent Event Filtering**: `calendar_find_meeting_times` filters out events with `status == "cancelled"` and transparent events (`transparency == "transparent"`) so they do not block available slots.
- **Exclusive All-Day Event Bounds**: Treats Google Calendar all-day event end date as exclusive (00:00:00 midnight of the end date instead of 23:59:59).
- **Pagination Support**: Added pagination loop requesting `nextPageToken` (capped at 10 pages) ensuring full coverage of calendar busy intervals across the requested time window.
- **Past Slot Filtering**: Filters out candidate meeting slots starting in the past with a 15-minute lead time buffer.

### 4. P1 — Guaranteed Routine Run Terminal States & Provider Neutrality (`libkyro/src/teach_task/scheduler.rs`)
- **Guaranteed Terminal Status in SQLite**: Refactored `execute_routine_unified` so that ANY failure during prompt or skill execution (vault read failure, unconfigured API key, agent initialization failure, or cancellation) reliably updates both `routine_runs` and `agent_runs` to terminal statuses (`failed` or `cancelled`) with recorded error messages.
- **Provider Neutrality**: Replaced hardcoded NVIDIA fallback (`keys.nvidia.unwrap_or_default()`) with generic provider key resolution (`vault.get_secret(&format!("{}_api_key", caps.provider))` or env variable), failing explicitly with clear user guidance if the selected model's provider is unconfigured.
- **Persistence Error Propagation**: Validated and logged database update results from `db.update_routine_run` and `db.update_run_status`.

### 5. P1 — Revoked-Folder Document Index Pruning (`libkyro/src/commands.rs`, `libkyro/src/main.rs`)
- **Folder Revocation Pruning**: `remove_authorized_folder` now immediately queries remaining authorized folders and invokes `db.prune_unauthorized_indexed_documents(&remaining_folders)`, deleting all indexed document records, cached text, and metadata for un-authorized paths.
- **Startup Reconciliation**: App initialization in `main.rs` setup now calls `db.prune_unauthorized_indexed_documents` against all currently authorized folders at launch.

### 6. P1 — Filesystem Authorization & TOCTOU Hardening (`libkyro/src/storage/organizer.rs`, `file_assistant.rs`)
- **Immediate Re-Canonicalization**: In `FileOrganizer::execute`, `op.source_path` is canonicalized immediately before moving and verified against `canonical_root` to eliminate time-of-check/time-of-use symlink swaps. Destination directory is verified post-creation with `canonicalize` and containment check. Destination collision check inspects symlink metadata to prevent writing through symlinks.
- **Search Symlink Confinement**: In `FileAssistant::collect_searchable_files`, symlinks are followed only if their canonical target stays within `canonical_root`, and canonicalized paths are pushed to the search index.

### 7. P1 — Study Assistant Topic Filtering (`kyroui/components/MyToolsView.tsx`)
- Added optional topic filter input in the Study Notes & Revision Flashcards tool.
- Bound input to `generate_study_notes` IPC call with `topic: studyTopic.trim() || null`, enabling targeted study note generation and concept extraction.

### 8. Final Blockers Remediation & Verification Phase
- **P0 Filesystem Authorization & Overwrite Safety**:
  - Re-canonicalization and containment verification executed on both source and destination paths immediately before copy/move/undo operations.
  - Hardened `FileAssistant::execute_safe_op` against silent destination clobbering: detects existing destinations via `exists()` and `symlink_metadata()` and requires explicit `confirm_destructive: true` confirmation before overwriting; safely pre-removes destinations when confirmed to avoid writing through symlinks.
  - Documented POSIX TOCTOU boundary: kernel-level race resistance relies on containment verification right before syscalls and pre-removal of existing entries.
  - Tested dynamic path replacement races in `test_dynamic_toctou_path_race_rejection_and_documentation`, move/copy collision in `test_filesystem_move_and_copy_destination_collision_and_confirm`, and undo containment/overwrite guards in `test_file_organizer_undo_containment_and_overwrite_safety`.
- **P0 Routine Persistence Parity & Startup Reconciliation**:
  - Unified routine runner in `RoutineScheduler::execute_routine_unified` verifies and propagates all initial agent run creation, terminal status persistence, and routine last-run metadata updates, never masking SQLite write failures as successes.
  - Overhauled `Database::reconcile_active_runs_on_startup` to transition orphaned `routine_runs` with `status = 'running'` to `'interrupted'` with accurate timestamps and error descriptions, and included routine count in total reconciled records. Verified in `test_startup_reconciliation_cleans_running_routine_runs`.
- **P1 OAuth State Handling & Account Lifecycle**:
  - Overhauled `ConnectorManager::is_permanent_refresh_error`: permanent failures (`invalid_grant`, `invalid_client`, `unauthorized_client`, `access_denied`, `revoked`, `status 400`, `status 401`, `status 403`, `status 404`, `not found`) transition accounts to `needs_reauth`; transient errors (DNS, timeouts, connection refused, 429, 502, 503) transition accounts to `error` without clearing tokens.
  - Redacted client secrets and refresh tokens from persisted error messages.
  - Updated `ConnectionsModal.tsx` to render the canonical `NEEDS REAUTH` badge and actionable "Reauthorize" button. Tested in `test_oauth_refresh_transient_vs_permanent_classification` and vitest.
- **P1 Timezone-Correct Calendar Availability**:
  - Exposed `timezone`, `work_start_hour`, and `work_end_hour` in `calendar_find_meeting_times` tool schema.
  - Added strict IANA timezone validation with `chrono_tz` and rejected inverted/invalid working hours (`start_hour >= end_hour`) before any network egress. Verified in `test_calendar_invalid_timezone_rejection_and_working_hours`.
  - Added pagination safety check returning explicit incomplete-data error when maximum page bound is reached.
- **P1 Unicode-Safe Document Grounding & Explanation**:
  - Overhauled `FileAssistant::find_unicode_passage` and `explain_passage` to eliminate arbitrary byte slicing, using `char_indices()` to guarantee valid UTF-8 boundaries and character-by-character Unicode case-folded matching.
  - Tested multilingual contracts with emoji (🚀), French accents (é), Cyrillic (Привет), Japanese CJK (日本語), and German umlauts (ü) in `test_unicode_multibyte_document_explanation_safety`.
- **P1 Dashboard Truthfulness & Privacy Disclosures**:
  - Updated `TodayDashboardView.tsx` to render explicit error banners for `calendar_error` and `email_error` with retry actions, eliminating deceptive empty states.
  - Updated `MyRoutinesView.tsx` to disable routine scheduling when clarification is required, providing quick-clarification buttons ("Every day", "Weekdays only", "Run once") and plain English schedule previews. Tested in `test_nl_routine_clarification_for_ambiguous_prompts` and vitest.
  - Updated `SettingsModal.tsx` privacy copy to accurately distinguish local file parsing and indexing from remote model inference over HTTPS.
- **P1 Revoked-Folder Pruning & Fail-Closed Document Access**:
  - Added `Database::remove_authorized_folder_and_prune` to atomically delete authorization records and purge un-authorized indexed document records and metadata.
  - Hardened `get_indexed_document` with a fail-closed check returning `Ok(None)` if the requested document path is outside currently authorized folders. Tested in `test_revoked_folder_and_prune_removes_authorization_and_indexed_data`.

### 9. Final End-to-End Verification Results
- `cargo fmt --manifest-path libkyro/Cargo.toml --all -- --check`: **PASS** (clean, zero formatting differences).
- `cargo clippy --manifest-path libkyro/Cargo.toml --all-targets --all-features -- -D warnings`: **PASS** (clean, zero warnings).
- `cargo test --manifest-path libkyro/Cargo.toml --all-features`: **PASS** (251 total backend tests passed, 0 failed):
  - 146 unit tests in `libkyro/src/lib.rs`
  - 18 agent loop tests in `libkyro/tests/agent_loop_tests.rs`
  - 17 command tests in `libkyro/tests/command_tests.rs`
  - 25 connector integration tests in `libkyro/tests/connector_integration_tests.rs`
  - 27 everyday assistant tests in `libkyro/tests/everyday_assistant_tests.rs`
  - 6 execution and MCP tests in `libkyro/tests/execution_and_mcp_tests.rs`
  - 29 security tests in `libkyro/tests/security_tests.rs`
  - 8 storage persistence tests in `libkyro/tests/storage_persistence_tests.rs`
  - 3 telegram bridge tests in `libkyro/tests/telegram_bridge_tests.rs`
  - 2 UI audit tests in `libkyro/tests/ui_audit_tests.rs`
- Vitest suite (`npm test`): **PASS** (259 tests passed across 31 suites, 0 failed, &ge; 95% frontend coverage).
- TypeScript compiler (`npx tsc --noEmit`): **PASS** (zero errors).
- Frontend bundle (`npm run build`): **PASS** (clean production Vite build).
- Verification pipeline (`make verify`): **PASS** (`[PASS] All Kyro verification checks passed successfully!`).

---

## 2026-10-23: Kyro v0.1.1 Release & Documentation Refactoring

### 1. Test Suite Expansion & Quality Finalization (≥95% Coverage)
- **Frontend Coverage Expansion**:
  - Expanded Vitest test suites to 31 test files with 259 passing tests, achieving &ge;95% coverage across all frontend views, modals, HUD controls, settings, and workflows.
  - Resolved `tsconfig.json` test configuration by excluding test files from the production compiler output, ensuring clean production builds (`tsc --noEmit && vite build`).
- **Backend Quality & Linter Compliance**:
  - Resolved Clippy warnings across all test targets (`-D warnings`), including function argument counts and mutex guard scopes across async points.
  - Enforced strict `cargo fmt` formatting across all Rust modules and test files.
  - Validated complete backend test suite passing with 251 tests (146 unit tests + 105 integration/security/browser tests).

### 2. Version Bump to `0.1.1`
- **Core Package Manifests**:
  - Bumped `package.json` to `0.1.1`.
  - Bumped `package-lock.json` to `0.1.1`.
  - Bumped `libkyro/Cargo.toml` and `libkyro/Cargo.lock` to `0.1.1`.
  - Bumped `libkyro/tauri.conf.json` to `0.1.1`.
- **Connectors & Protocol Adapters**:
  - Updated User-Agent headers to `Kyro-Desktop-Agent/0.1.1` and `Kyro-Autonomous-Desktop-Agent/0.1.1`.
  - Updated MCP clientInfo version to `0.1.1` in `libkyro/src/mcp/client.rs` and `libkyro/src/connector/remote_mcp.rs`.
- **Installers & Packaging**:
  - Updated `install.sh` fallback and help text to `0.1.1`.
  - Updated `README.md` distro download links to `0.1.1` (`.deb`, `.rpm`, `.pkg.tar.zst`, `.AppImage`, `.tar.gz`).
  - Updated GitHub issue templates to reference `0.1.1`.

### 3. Documentation Refactoring
- **System Architecture Docs**:
  - Refactored `runtime-overview.md` to include `connector/` (Native GitHub & Google, Remote MCP, PKCE Auth) and `telegram/` subsystems, and eliminated duplicate module entries.
  - Refactored `security-boundaries.md` with comprehensive documentation of RFC 2822 email injection defenses, RFC 2047 word encoding, POSIX TOCTOU symlink re-canonicalization, folder revocation index pruning, and OAuth single-flight refresh synchronization.
  - Refactored `connector-system.md` to document the reference `GoogleAdapter` (Gmail draft/send and Google Calendar timezone-correct availability) alongside `GitHubAdapter`.
- **Implementation Status & Feature Matrix**:
  - Updated `feature-matrix.md` with frontend &ge;95% coverage verification and cross-distro packaging pipeline status.
  - Updated `remediation-plan.md` to record completion of Phases 14 and 15.

---

## 2026-10-24: Kyro v0.1.2 Release — Critical Agent Runtime, Security, Worktree Isolation & Registry Remediation

### 1. Security & Execution Boundary Remediation
- **Tool Authorization at Actual Execution Boundary**:
  - Centralized agent profile tool permission enforcement directly within `execute_tool_governed` before any tool dispatches.
  - Implemented `canonical_tool_name` normalization and alias resolution (`bash`, `sh`, `terminal`, `shell` → `execute_shell`).
  - Added strict policy validation for `allowlist` and `inherit` modes, failing closed on unknown modes or tools.
  - Enforced `delegation.enabled == false` checks at the dispatch boundary, rejecting forged `delegate_task` calls.

### 2. Workflow Worktree Containment & Reconciliation
- **Authoritative Execution Root**:
  - Bound subprocess working directory in `execute_tool_core` (`cmd_builder.current_dir(root)`) so shell commands and scripts run inside the isolated worktree.
  - Introduced fail-closed workspace containment: if an isolated worktree is requested in the run configuration but does not exist on disk, the task halts immediately instead of falling back to the project root.
  - Created collision-resistant worktree paths (`.kyro/worktrees/{workspace_id}_{task_id}_a{attempt}`) and Git branches (`kyro-wt-{workspace_id}-{task_id}-a{attempt}`).
  - Captured `base_commit` at worktree setup, discovered `merge-base` against project root `HEAD`, detected concurrent modifications on touched files, and flagged `can_safely_merge: false` on conflicts.
  - Added `apply_reconciliation` for safe branch merges and diff application.

### 3. Transactional Registry & Fallback Protection
- **Transactional Profile Saves**:
  - Implemented prospective uniqueness validation in `save_profile`: clones in-memory entries into a candidate registry, validates uniqueness across IDs, display names, and aliases prior to writing to disk.
  - Implemented atomic disk persistence via temporary file creation and atomic rename.
  - Rolled back in-memory state on validation failure.
  - Added built-in override fallback protection: preserves `override_error` diagnostics for corrupt override files while falling back to built-in defaults.

### 4. Bounded Multi-Agent Delegation & Scoped Memory
- **Delegation Safety**:
  - Prevented recursion cycles by walking the SQLite database parent-run ancestor chain.
  - Blocked self-delegation.
  - Treated database insertion of child runs as fatal, aborting delegation if persistence fails.
  - Respected profile `model.mode` (`fixed` vs `inherit`).
- **Scoped Fact Retrieval**:
  - Added `MemoryStore::build_context_block_scoped` to strictly filter facts according to profile `context.memory_scopes` (`preference`, `agent`).

### 5. Authoritative Mention Parsing & UI Synchronization
- **Mention Routing**:
  - Implemented `strip_non_routable_text` and `extract_mention_tokens` in `AgentRegistry`, ignoring code fences, blockquotes, inline backticks, and email addresses.
  - Implemented `route_turn` routing mentions to resolved profiles and falling back to default agent (`apex-orchestrator`) with informative diagnostics.
- **Frontend Teammate Roster**:
  - Updated origin badges to "Built-in", "Override", and "Custom" in `TeammateRosterModal.tsx`.
  - Added automatic fallback to Apex when deleting or disabling the currently active teammate.
  - Preserved draft edits and displayed validation errors in the modal upon save rejection.

### 6. Test Suite & Verification Results
- **Automated Tests**:
  - Authored 11 new end-to-end remediation verification tests in `libkyro/tests/remediation_verification_tests.rs`.
  - **313 backend Rust tests** passed (201 unit tests + 112 integration/security tests).
  - **268 frontend Vitest tests** passed across 33 test files (&ge;95% coverage).
  - Zero warnings under `cargo clippy --manifest-path libkyro/Cargo.toml --all-targets --all-features -- -D warnings`.
  - Clean `cargo fmt --manifest-path libkyro/Cargo.toml --all -- --check`.
  - Clean `npx tsc --noEmit && npm run build`.

### 7. Version Bump to `0.1.2`
- Bumped `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/tauri.conf.json`, `install.sh`, connectors, issue templates, and docs to `0.1.2`.

---

## 2026-10-25: Kyro v0.1.3 Release — Modern Shell Redesign, Live Reasoning Streaming, Hardened File Organizer & Multi-Provider Isolation

### 1. Application Shell Redesign & Reusable UI Component System
- **Modern Desktop Shell (`App.tsx`)**:
  - Rebuilt workspace shell with high-density sidebar navigation, conversation list, quick action buttons, and active model status.
  - Added zero-state welcome experience with prompt suggestion chips for rapid workflow initiation.
  - Implemented `TaskComposer` featuring multiline auto-expanding textarea, drag-and-drop file/image attachment previews, Enter-to-send (Shift+Enter for newline), and `@mention` agent selector.
  - Built modular UI components in `kyroui/components/ui/`: `Button`, `Dialog`, `CodePanel`, `StatusBadge`, `Surface`, and `TextField` with keyboard focus management and accessible ARIA attributes.
- **Per-Conversation Isolation**:
  - Preserved active task plans and execution state scoped by conversation ID, preventing cross-conversation state bleed and attachment race conditions.
  - Bounded OAuth polling with automatic cleanup on unmount in `ConnectionsModal.tsx`.

### 2. Live Model Reasoning Streaming & Typography
- **SSE Stream Extraction in `ModelRouter`**:
  - Enhanced SSE parser to capture real-time reasoning deltas and `<think>` blocks, strictly isolating reasoning tokens from user-facing assistant content and structured tool calls.
  - Scoped provider-specific parameters like `chat_template_kwargs.enable_thinking` strictly to compatible models (Nemotron).
  - Emitted `AgentReasoningEvent` with run, conversation, and delegated agent attribution.
- **Thinking Indicator & Monospace Typography**:
  - Replaced static indicator with `ThinkingIndicator` component featuring smooth dark-theme animations, auto-scrolling reasoning disclosures, honest waiting fallbacks, and `prefers-reduced-motion` compliance.
  - Adopted 12px `@fontsource/fira-code` across code panels, `ToolCallCard`, terminal views, and markdown code blocks.

### 3. Hardened Safe Reversible File Organizer
- **Server-Authoritative Anti-Tampering & Freshness**:
  - Introduced unified `SharedFileOrganizer` orchestrator backing both UI commands and agent tools.
  - Persisted candidate plans to `file_organization_plans` table; execution strictly requires an existing, unexecuted plan record.
  - Performed pre-move SHA-256 freshness verification; stale plans or modified source files trigger immediate abort.
  - Excluded symlinks during scanning to defend against traversal attacks, reporting symlinks as conflicts.
  - Enforced authorized folder boundary confinement, resolving `~` paths safely and failing closed on unauthorized paths.
- **Durable File Transactions & Undo Recovery**:
  - Journaled multi-file movements into `file_transactions` table with pre/post hash state.
  - Registered `undo_file_transaction` and `get_file_transaction` Tauri commands, enabling reliable one-click undo recovery.
  - Enforced human-in-the-loop governance: mutating filesystem operations require explicit user authorization regardless of persisted permission level.

### 4. Native Anthropic Messages API & Router Hardening
- Implemented native Anthropic Messages API client with system prompt handling, streaming event handling, and provider classification in `ModelRouter`.
- Isolated sub-routes and child tasks to prevent context pollution between parallel workflows.

### 5. Storage CAS Guards & Governance Provenance
- Enforced Compare-And-Swap (CAS) guards on agent run status updates in SQLite, making terminal outcomes (`succeeded`, `failed`, `cancelled`, `interrupted`) immutable against late-arriving completion race conditions.
- Preserved task grant provenance to prevent privilege hijacking; enforced negation handling in governance engine.
- Hardened dynamic connector policy: prevented dynamic tools from self-authorizing through naming heuristics, requiring explicit local policy configuration.
- Enforced strict UTF-8 validation and binary magic byte detection (ELF, Mach-O, PE, ZIP, GZIP) on text attachments.

### 6. Test Suite & Verification Results
- **Automated Tests**:
  - Added new regression test suites for task-plan truthfulness, attachment policy, run lifecycle ownership, and OAuth polling lifecycle.
  - **317 backend Rust tests** passed with zero warnings under `cargo clippy --all-targets --all-features -- -D warnings`.
  - **318 frontend Vitest tests** passed across 39 test files (&ge;95% coverage).
  - Clean `cargo fmt --manifest-path libkyro/Cargo.toml --all -- --check`.
  - Clean production frontend build with `npm run build`.

### 7. Version Bump to `0.1.3`
- Bumped `package.json`, `package-lock.json`, `libkyro/Cargo.toml`, `libkyro/Cargo.lock`, `libkyro/tauri.conf.json`, `install.sh`, connectors, issue templates, and docs to `0.1.3`.
