# Kyro Implementation Feature Matrix

This matrix tracks the authoritative implementation and verification status of all capabilities in Kyro. Status values: `not_started`, `in_progress`, `implemented`, `verified`, or `blocked`.

| Subsystem / Capability | Priority | Status | Verification Evidence / Test |
| :--- | :--- | :--- | :--- |
| **Centralized Authorization Engine** | P0 | `verified` | `test_centralized_authorization_engine`, `test_permission_gate`, `test_authorization_workspace_confinement_and_traversal` |
| **Workspace Confinement & Path Security** | P0 | `verified` | `validate_path_confinement` with `FileOp` (`Read`, `Write`, `Create`, `Delete`), rejects traversal `..`, null bytes, unauthorized absolute paths, symlink escapes |
| **Hardened Shell Evaluation** | P0 | `verified` | `test_authorization_shell_injection_multiline_and_compounds` (rejects multiline injection, subshells, pipelines, chaining, wrappers) |
| **Fail-Closed Container Sandbox** | P0 | `verified` | `test_sandbox_probe` (fails closed when container engine absent; `SandboxOnly` never falls back to host) |
| **Durable Approval Queue & CAS Consumption** | P0 | `verified` | `test_approval_expiry_and_atomic_consumption` (atomic CAS prevent double-execution; background expiration) |
| **Direct Computer-Action Policy Gate** | P0 | `verified` | `perform_computer_action` validates `AuthorizationEngine::evaluate_computer_action` before execution |
| **Secure Vault & Keyring Migration** | P0 | `verified` | `test_vault_migration_and_secure_storage`, `test_database_and_vault` (live keyring probe, 0600 file fallback, authenticated AES-256-GCM) |
| **Zero Secret Leakage to Frontend** | P0 | `verified` | `get_api_keys` returns `ApiKeysMetadata` without plaintext secrets |
| **Durable Agent Runs & Lifecycle** | P0 | `verified` | `test_durable_agent_runs_and_events`, `test_end_to_end_agent_task_lifecycle_and_recovery` |
| **Startup Crash Reconciliation** | P0 | `verified` | `reconcile_active_runs_on_startup` marks orphaned runs as `interrupted` |
| **Run-Scoped Cancellation & Process Tree Cleanup** | P0 | `verified` | `test_cancellation_token` (per-run token, process group PGID SIGKILL, HTTP abort) |
| **Accurate Cancellation Reporting** | P0 | `verified` | `cancel_agent_run` checks existence, refuses to overwrite terminal states (`succeeded`/`failed`/`completed`) |
| **Multimodal Vision Pipeline** | P1 | `verified` | `test_multimodal_message_content` (preserves base64/mime type, passes `MessageContent::Parts` end-to-end) |
| **Set-of-Marks Visual Grounding** | P1 | `verified` | `test_set_of_marks_grounding`, `click_mark` tool |
| **Coordinate Space Transformation** | P1 | `verified` | `test_coordinate_transform` (multi-monitor scaling) |
| **Ghost Mode Virtual Display (:99)** | P1 | `verified` | `GhostDisplayManager`, virtual X11 server isolation |
| **Visual Step Timeline & Real Diffs** | P1 | `verified` | `test_visual_verification_real_pixel_diff` (actual pixel diff computation via `TimelineEngine::compute_diff_percent`) |
| **Model Router & Exponential Retry** | P1 | `verified` | `ModelRouter::complete_with_retries` with 429/503 backoff; rejects image attachments on non-vision models |
| **MCP Client & Tool Discovery** | P1 | `verified` | `test_mcp_untrusted_names_and_schema_validation` (strict schema validation against `inputSchema`, server-scoped names `mcp__{server_id}__{tool}`) |
| **Teach-a-Task Skill Recorder** | P1 | `verified` | `test_teach_task_recorder`, credential sanitization filter |
| **Deterministic Skill Replay** | P1 | `verified` | `SkillReplayer::execute` with governance evaluation on each action step |
| **Self-Healing & Parameterization** | P1 | `verified` | `test_self_healing_and_parameterization` (coordinate delta & substitution) |
| **Cron & Interval Routine Scheduler**| P1 | `verified` | `test_scheduler_timezone_and_overlap_guard` (IANA `chrono-tz` support, DST handling, missed run policy >24h) |
| **Scoped Persistent Memory** | P1 | `verified` | `test_scoped_memory_relevance_ranking_and_budget` (keyword overlap scoring, recency boost, scope isolation, `MAX_MEMORY_CHARS` 2500 cap) |
| **Multi-Agent Roster & Delegation** | P1 | `verified` | `test_multi_agent_roster_and_delegation` (passes `botId` from UI, generates unique child run IDs, depth bound <= 3, drop-guard status tracking) |
| **Mention Detection (@Apex, @Forge)**| P1 | `verified` | `test_multibot_mention_detection` |
| **Installed Desktop App Indexer** | P1 | `verified` | `test_app_indexer` (parses Linux `.desktop` files) |
| **Durable Artifacts Registry** | P1 | `verified` | `test_artifacts_lifecycle_and_path_security`, `ArtifactsModal.tsx` |
| **Universal Workspace Search** | P2 | `verified` | `test_universal_search`, `CommandPalette.tsx` |
| **Command Palette (`Ctrl+K`)** | P2 | `verified` | `CommandPalette.tsx` keyboard navigation & quick actions |
| **Zero-Cloud Voice Dictation** | P2 | `verified` | Web Speech API integration in `App.tsx` |
| **Local Text-to-Speech (TTS)** | P2 | `verified` | `window.speechSynthesis` audio playback on assistant messages |
| **Prompt File & Image Attachments** | P2 | `verified` | Full binary/base64 attachment preserved and dispatched as `MessageContent::Parts` |
| **Desktop Notifications** | P2 | `verified` | Web Notification API dispatched on task finish and approvals |
| **Portable Templates & Ecosystem** | P2 | `verified` | `test_workspace_templates_export_and_import` (secret redaction verified) |
| **Starter Template Library** | P2 | `verified` | DevOps & Web QA starter packs in `TemplateEngine` |
| **System Diagnostics & Health Probe**| P2 | `verified` | `get_system_health` (probes WAL, Docker, Xvfb, Keyring, CLI tools) |
| **Interactive ANSI PTY Terminal** | P1 | `verified` | `PtySession` via `portable-pty` connected to xterm.js |
| **Telegram Remote Bridge** | P1 | `verified` | `TelegramBridge` fail-closed authentication (rejects empty or mismatched `allowed_chat_id`), dispatches tasks to `AgentLoopEngine`, mobile approvals |
| **Floating HUD Widget** | P1 | `verified` | `HudPanel.tsx`, `toggle_hud_window` |
| **Screen Region Snipper** | P1 | `verified` | `Snipper.tsx`, `take_region_screenshot` |
| **Authoritative Browser Capability Registry** | P0 | `verified` | `browser_capabilities_reports_supported_and_unsupported_browsers`, platform discovery of Firefox, Chrome, Chromium, Edge, Brave, Vivaldi, Opera |
| **Provider-Neutral Browser Automation Interface** | P0 | `verified` | `worker.mjs` native WebSocket protocol adapter supporting WebDriver BiDi (Firefox) and CDP (Chromium/Brave/Edge/Chrome) with session isolation |
| **Strict Browser Selection Priority & Non-Substitution** | P0 | `verified` | `kyro_does_not_silently_substitute_another_browser`, `unsupported_browser_rejection_names_alternatives` (fails closed, lists available alternatives) |
| **Dynamic Project Discovery & Resolution** | P0 | `verified` | `test_ui_audit_reports_missing_projects_honestly`, `plan_rejects_missing_project_name_and_path`, `plan_uses_explicit_project_path`, `unrelated_projects_are_not_included_in_candidates` |
| **Autonomous UI Audit Workflow** | P1 | `verified` | `test_ui_audit_finds_the_known_defects` (verifies dev server ready URL, routes, viewports, reproducible console/network/form/image/responsive bugs) |
| **Browser Preferences & Settings UI** | P1 | `verified` | `browser_settings.test.tsx`, `SettingsModal.tsx` Browser tab, `save_browser_settings`, `get_browser_settings` |
