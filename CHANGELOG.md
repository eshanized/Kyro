# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.3] - 2026-10-25

### Modern Application Shell, Live Reasoning Streaming, Hardened File Organizer & Multi-Provider Isolation

#### User Interface & Experience Redesign
- **Application Shell & Sidebar Navigation**: Redesigned modern, high-density desktop shell with intuitive sidebar navigation, conversation switcher, quick action shortcuts, and active model status.
- **Welcome Home Experience & Prompt Chips**: Added zero-state welcome screen with interactive quick-prompt suggestion chips for instant onboarding across everyday and developer tasks.
- **Multiline Auto-Expanding Composer**: Built `TaskComposer` with auto-expanding multiline text input, drag-and-drop file/image attachment previews, Enter-to-send (Shift+Enter for newline), and active agent mention routing.
- **Reusable Component System**: Implemented modular, accessible design system components in `kyroui/components/ui/` (`Button`, `Dialog`, `CodePanel`, `StatusBadge`, `Surface`, `TextField`) with keyboard focus trapping and ARIA compliance.
- **Per-Conversation State Preservation**: Preserved active task plans and execution state across conversation switches, preventing state leakage and attachment race conditions.

#### Live Model Reasoning & Typography
- **SSE Reasoning Stream Extraction**: Enhanced `ModelRouter` to extract streaming provider-exposed reasoning tokens (`<think>` blocks and reasoning deltas) in real-time while keeping thinking traces strictly separate from assistant response content and tool call definitions.
- **Thinking Indicator & Panel**: Implemented `ThinkingIndicator` component featuring dark-theme wave and shimmer animations, smooth auto-scrolling reasoning disclosures, honest waiting fallbacks, and `prefers-reduced-motion` compliance.
- **12px Fira Code Typography**: Adopted `@fontsource/fira-code` at 12px across code blocks, tool calls (`ToolCallCard`), markdown renders, and terminal output.

#### Safe Reversible File Organizer Hardening
- **Server-Authoritative Anti-Tampering**: Introduced `SharedFileOrganizer` orchestrator with server-authoritative plan validation backed by SQLite `file_organization_plans` table, guaranteeing that only pre-calculated and validated plans can be executed.
- **Pre/Post Move SHA-256 Freshness Verification**: Verified SHA-256 hashes immediately before moving files; stale plans or externally modified files are automatically aborted to prevent data loss.
- **Symlink Exclusion & Conflict Detection**: Detected symlinks during scanning, excluded them from destructive moves to prevent symlink traversal attacks, and surfaced them in preview conflict reports.
- **Authorized Folder Confinement & Directory Traversal Prevention**: Confined all file scanning and moves strictly within user-authorized directories, resolving `~` home directory paths safely and rejecting unauthorized traversal.
- **Durable File Transactions & Undo Recovery**: Persisted movements to `file_transactions` table with pre/post hash state, enabling complete, atomic one-click undo recovery (`undo_file_transaction`, `get_file_transaction`).
- **Human-in-the-Loop Governance**: Required explicit human confirmation for all mutating file reorganization actions regardless of persisted permission level.

#### Model Routing & Provider Architecture
- **Native Anthropic Messages API**: Implemented native Anthropic Messages API client in `ModelRouter` with system prompt parameter handling, streaming support, and provider error classification.
- **Child Route Isolation**: Isolated sub-routes and delegated tasks to prevent context pollution between parallel agent workflows.
- **Model-Specific Parameter Scoping**: Scoped provider-specific arguments (such as `chat_template_kwargs.enable_thinking`) strictly to compatible models (e.g. Nemotron).

#### Governance, Storage & Security Hardening
- **Storage CAS Status Guards**: Enforced Compare-And-Swap (CAS) guards on agent run status updates in SQLite, making terminal outcomes (`succeeded`, `failed`, `cancelled`, `interrupted`) immutable against late-arriving completion race conditions.
- **Grant Provenance & Negation Handling**: Enforced task grant provenance, binding grants to their originating task and preventing cross-session privilege escalation. Handled permission negation policies correctly.
- **Connector Dynamic Policy Hardening**: Prevented external dynamic tools from self-authorizing through naming heuristics alone; required explicit local user policy configuration.
- **Strict Attachment UTF-8 & Binary Magic Byte Validation**: Validated text attachments for strict UTF-8 compliance and detected binary magic byte signatures (ELF, Mach-O, PE, ZIP, GZIP) to prevent binary smuggling.
- **Bounded OAuth Polling**: Bounded background OAuth authentication polling attempts with automatic timer cancellation and cleanup on component unmount.

#### Quality Assurance & Verification
- **Rust Verification Suite**: 317 passing tests across unit, integration, and security suites with 0 warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- **Frontend Test Suite**: 318 passing tests across 39 Vitest suites with &ge;95% code coverage.

---

## [0.1.2] - 2026-10-24

### Critical Agent Runtime, Security, Worktree Isolation & Registry Remediation

#### Security & Policy Enforcement
- **Execution Boundary Agent Tool Authorization**: Centralized profile-level tool permissions inside `execute_tool_governed` before any tool call executes, ensuring mocked or forged model requests cannot invoke disallowed tools.
- **Canonical Tool Name Normalization**: Added `canonical_tool_name` resolution mapping shell aliases (`sh`, `bash`, `terminal`, `shell` → `execute_shell`) to prevent alias-based permission bypass.
- **Forged Delegation Rejection**: Blocked `delegate_task` calls immediately at the execution boundary when an agent's profile has `delegation.enabled = false`.
- **Fail-Closed Policy Resolution**: Unknown tool modes or unresolvable profile policies fail closed without executing the requested operation.

#### Workflow Isolation & Git Worktree Management
- **Authoritative Execution Root**: Propagated `cmd_builder.current_dir(root)` in `execute_tool_core` so shell processes and scripts explicitly run in their assigned isolated workspace.
- **Fail-Closed Workspace Containment**: Tasks halt immediately with an informative error if an isolated worktree was configured but is missing on disk, eliminating silent fallback to the main repository.
- **Collision-Resistant Worktrees**: Generated unique worktree directories (`.kyro/worktrees/{workspace_id}_{task_id}_a{attempt}`) and Git branches (`kyro-wt-{workspace_id}-{task_id}-a{attempt}`) to prevent parallel tasks and retries from colliding.
- **Base-Commit Conflict Detection**: Recorded `base_commit` at worktree setup, identified `merge-base` against project root `HEAD`, detected concurrent modifications on touched files, and flagged `can_safely_merge: false` on conflicts.
- **Reconciliation Engine**: Added `apply_reconciliation` for safe non-fast-forward branch merging and diff application to project root.

#### Agent Registry & Profile Persistence
- **Transactional Profile Saves**: Implemented prospective cross-profile uniqueness validation (display names, aliases, IDs) on candidate registry entries prior to writing to disk.
- **Atomic Persistence & Rollback**: Persisted profile changes using temporary files with atomic rename; rolling back in-memory state on prospective validation failure.
- **Built-in Override Diagnostics**: Preserved `override_error` diagnostics for invalid override manifests on disk while falling back gracefully to embedded defaults.

#### Delegation Safety & Scoped Context Memory
- **Recursion Cycle Prevention**: Prevented infinite delegation loops by walking the SQLite parent run ancestor chain and blocked self-delegation.
- **Fatal Child Run Persistence**: Made child run database insertion fatal, halting delegation immediately if persistence fails.
- **Model Resolution**: Enforced `model.mode` semantics (`fixed` vs `inherit`).
- **Scoped Fact Retrieval**: Added `MemoryStore::build_context_block_scoped` to filter context facts strictly by configured `context.memory_scopes` (`preference`, `agent`).

#### Natural Language Mention Parsing & UI Synchronization
- **Context-Stripped Mentions**: Added `strip_non_routable_text` and `extract_mention_tokens` in `AgentRegistry` to ignore mentions inside code fences, blockquotes, inline backticks, and email addresses.
- **Fallback Routing**: Routed turns to default `apex-orchestrator` with diagnostic reasons when requested or mentioned agents are unavailable.
- **Teammate Roster UI Badges**: Added clear origin badges ("Built-in", "Override", "Custom") in `TeammateRosterModal.tsx`.
- **Active Selection Fallback**: Automatically fall back to Apex and persist the selection if an active teammate is deleted or disabled.
- **Draft Retention**: Preserved form draft edits and surfaced validation diagnostics on backend save rejection.

#### Testing & Quality Assurance
- **Comprehensive Rust Verification**: Expanded backend suite to **313 tests** passing with zero warnings under `cargo clippy --all-targets --all-features -- -D warnings`.
- **Frontend Test Suite**: Maintained **268 passing tests** across 33 Vitest test files with &ge;95% coverage.

---

## [0.1.1] - 2026-10-23

### Quality Assurance Expansion, Communications Hardening & Distro Packaging

#### Communications & Security Hardening
- **RFC 2822 Email Header Injection Defense**: Strict rejection of CRLF, NULL, or control characters across `to`, `subject`, and `in_reply_to` fields.
- **RFC 2047 MIME Word Encoding**: Base64 encoding for Unicode display names and non-ASCII subjects.
- **Provider-Specific Single-Flight OAuth Refresh**: Dynamic token refresh for Google and GitHub with mutex synchronization and transient vs permanent error classification.
- **Timezone-Correct Calendar Availability**: Strict IANA timezone evaluation using `chrono-tz`, filtering cancelled and transparent events with configurable working hours.
- **POSIX TOCTOU Hardening**: Re-canonicalization and containment checks immediately prior to filesystem copy/move/undo operations.
- **Revoked Folder Index Pruning**: Automatic atomic purging of indexed documents outside authorized directories.
- **Unicode Document Grounding**: Character-safe passage extraction preserving valid UTF-8 boundaries across emoji, Cyrillic, CJK, and accented text.

#### Everyday Assistant & Workflows
- **Everyday Destinations**: Added `Today`, `My Tools`, and `My Routines` views with structured task planning.
- **FileAssistant Module**: Natural language document indexing and search.
- **Local Habit Learning**: Privacy-first opt-in habit detection and routine suggestion engine.
- **Sandboxed Experiments**: Containerized experiment execution with 64KB bounded logs and diff comparisons.

#### Automated Packaging & CI
- **Cross-Distro Linux Packages**: Automated building of `.deb`, `.rpm`, `.pkg.tar.zst`, `.AppImage`, and portable `.tar.gz` packages with SHA256 verification.
- **Test Coverage Expansion**: Reached 259 Vitest frontend tests across 31 test files (&ge;95% coverage) and 251 backend Rust tests.

---

## [0.1.0] - 2026-10-22

### Initial Public Release of Kyro

- **Local-First Autonomous Desktop AI Agent**: Built with Tauri v2, Rust, and React 19.
- **Centralized Authorization Engine**: Granular policy evaluation for shell, computer-use, filesystem, and connectors.
- **OS Keyring Vault**: Secure credential storage using OS keyring with AES-256-GCM fallback.
- **Computer-Use & Visual Grounding**: Set-of-Marks (SoM) visual coordinate tagging and native input synthesis.
- **Model Context Protocol (MCP)**: Full client implementation supporting stdio, HTTP JSON-RPC 2.0, and SSE transports.
- **Interactive HUD & Snipper**: Floating overlay window with instant search and region selector.
- **Telegram Remote Bridge**: Bidirectional notification and mobile task execution bridge.
