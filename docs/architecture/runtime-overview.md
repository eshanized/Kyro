# Kyro Runtime Architecture Overview

## 1. System High-Level Topology

Kyro is a local-first, autonomous desktop agent application built on Tauri 2, featuring a React 19 + TypeScript frontend and a multi-threaded Rust backend (`libkyro`).

```mermaid
graph TD
    UI["React 19 / Vite UI (Desktop + HUD + Overlay + Command Palette)"]
    Tauri["Tauri 2 IPC Bridge (Commands & Events)"]
    Engine["AgentLoopEngine (Multi-step Task Orchestration)"]
    Auth["Centralized Security & Authorization Engine"]
    Router["Model Provider Router (NVIDIA NIM / Multimodal)"]
    CU["Computer-Use Subsystem (xcap, enigo, SoM Grounding)"]
    PTY["Terminal / PTY Session Manager"]
    Sandbox["Container / Sandbox Isolation (Docker/Podman)"]
    MCP["Model Context Protocol (MCP) Client Manager"]
    Conn["ConnectorManager (Native APIs, Remote MCP, Local MCP)"]
    DB["SQLite Storage & Versioned Migrations (WAL Mode)"]
    Vault["OS Keyring / Authenticated Encrypted Vault Service"]
    Routine["Routine Scheduler (Cron / Intervals)"]
    Tele["Telegram Remote Bridge"]
    Mem["Persistent Memory & App Indexer"]
    Multi["Multi-Agent Orchestrator (Apex, Forge, Scout)"]
    Art["Local Artifacts Registry & Templates Engine"]

    UI <--> |IPC Invoke & Events| Tauri
    Tauri --> Engine
    Tauri --> Conn
    Engine --> Auth
    Auth --> |Authorized Tool Execution| CU
    Auth --> |Authorized Tool Execution| PTY
    Auth --> |Authorized Tool Execution| Sandbox
    Auth --> |Authorized Tool Execution| MCP
    Auth --> |Authorized Tool Execution| Conn
    Auth --> |Authorized Tool Execution| Art
    Conn --> Vault
    Conn --> DB
    Engine <--> Router
    Engine <--> Mem
    Engine <--> DB
    Tauri --> Routine
    Tauri --> Tele
    Routine --> Engine
    Tele --> Engine
    Tauri --> Vault
    Vault --> DB
    Engine --> Multi
    Tauri --> Art
    Art --> DB
```

## 2. Component Inventory

### 2.1 Frontend Modules (`kyroui/`)
- `App.tsx`: Main workspace view displaying conversation threads, live step timeline, tool execution cards, screen preview, model status, voice dictation, file attachments, and prompt dispatch.
- `hud/HudPanel.tsx`: Compact floating HUD window (`label: hud`) for autonomous task execution and quick snipping.
- `hud/Snipper.tsx`: Interactive screenshot region selector for visual grounding.
- `components/CommandPalette.tsx`: Global instant command palette (`Ctrl+K`) with multi-category universal search across conversations, messages, memories, skills, routines, and artifacts.
- `components/ArtifactsModal.tsx`: Local artifact registry drawer for previewing code, markdown, logs, json, and exporting generated files.
- `components/SettingsModal.tsx`: Configuration for NVIDIA API keys, Telegram credentials, governance matrix policy, model selections, extensible template packs, and system diagnostics.
- `components/ConnectionsModal.tsx`: Connections and app integrations modal: Discover Catalog, Connected Accounts, Custom Local MCP, and per-action governance policy manager.
- `components/TeammateRosterModal.tsx`: Agent team inspector (Apex, Forge, Scout) showing roles, capabilities, status, and task assignments.
- `components/TeachTaskModal.tsx`: Teach-a-Task recorder, skill catalog, and routine scheduler.
- `components/TimelineDrawer.tsx`: Historical timeline drawer visualizing step actions, coordinates, and pixel differences.
- `components/TerminalView.tsx`: Interactive xterm.js terminal connected to host PTY.
- `components/ToolCallCard.tsx`: Live card rendering tool calls, arguments, outputs, and status.
- `types/agent.ts`: Type definitions for domain entities, IPC payloads, and events.

### 2.2 Backend Modules (`libkyro/src/`)
- `main.rs`: Application entry point, config directory resolution, subsystem initialization, and Tauri command registration.
- `commands.rs`: Typed Tauri IPC command handlers (50+ commands covering execution, governance, memory, artifacts, search, and diagnostics).
- `agent/`:
  - `loop_engine.rs`: Multi-step agent execution loop, tool definitions, tool dispatch, step events, per-run cancellation, artifact generation.
  - `router.rs`: Model client for NVIDIA NIM / OpenAI-compatible endpoints with multimodal content parts and exponential backoff retry.
  - `cancellation.rs`: Scoped cancellation tokens with child run propagation.
- `governance/`:
  - `auth.rs`: Centralized `AuthorizationEngine` evaluating shell modes, computer use modes, blocked domains, path boundaries, credential checkpoints, and approval queues.
  - `policy.rs`: Policy engine (`GovernancePolicy`, blocked domains, shell modes).
  - `takeover.rs`: Sensitive checkpoint detection (2FA, CAPTCHA, passwords) and takeover status.
- `execution/`:
  - `permissions.rs`: Command risk assessment and permission gating.
  - `pty.rs`: Pseudo-terminal (PTY) spawn, resize, and I/O handling via `portable-pty`.
- `computer_use/`:
  - `capture.rs`: Screen and region screenshot capture using `xcap`.
  - `input.rs`: Mouse and keyboard synthesis via `enigo`.
  - `overlay.rs`: Coordinate transformation and multi-monitor scaling.
- `grounding/`:
  - `som.rs`: Set-of-Marks visual grounding engine, tag badge rendering, mark resolution.
- `ghost/`:
  - `display.rs`: Isolated virtual display (`:99`) control and application launch.
- `storage/`:
  - `db.rs`: SQLite database connection management, migrations (v1-v5), queries for conversations, messages, skills, routines, bots, MCP servers, runs, events, memories, timeline steps, artifacts, connector definitions, connected accounts, and connector tool policies.
  - `vault.rs`: Credential and API key management via OS Keyring and authenticated AES-256-GCM encrypted database fallback, with namespaced connector account secrets.
  - `templates.rs`: Extensible template packs, schema versioning, secret sanitization, and pre-packaged starters.
- `connector/`:
  - `catalog.rs`: Built-in catalog specifications for Native APIs and MCP connectors.
  - `auth.rs`: PKCE OAuth state generation, cryptographic verification, and local callback listener.
  - `manager.rs`: Centralized connector runtime orchestrating native adapters, remote MCP clients, local MCP servers, and tool execution.
  - `remote_mcp.rs`: Remote MCP client over HTTP JSON-RPC 2.0.
  - `native/`: Native API adapters (including `GitHubAdapter` with 11 tools).
- `mcp/`:
  - `client.rs`: Stdio JSON-RPC transport for Model Context Protocol servers.
  - `manager.rs`: Server lifecycle, discovery, schema validation, and tool execution.
- `teach_task/`:
  - `recorder.rs`: Task recording session manager with credential sanitization.
  - `replay.rs`: Deterministic workflow replayer with step progress reporting.
  - `scheduler.rs`: Routine background worker for cron/periodic execution.
  - `self_healing.rs`: Parameter substitution and coordinate recalibration.
- `timeline/`:
  - `diff.rs`: Frame-to-frame pixel diff calculation, click marker annotation, and step tracking.
- `memory/`:
  - `indexer.rs`: Installed desktop application indexer (`.desktop` parsing).
  - `store.rs`: Scoped context block builder (preferences, agent facts, project memory) and relevance retrieval.
- `multibot/`:
  - `bot.rs`: Bot profile definitions.
  - `group.rs`: Mention parsing and group system prompt generation.
- `telegram/`:
  - `bridge.rs`: Long-polling worker for Telegram bot command and notification exchange.
- `sandbox/`:
  - `docker.rs`: Container isolation detection and CLI execution.
- `browser/`:
  - `mod.rs`: `BrowserAutomationManager`, authoritative browser capability registry, browser preferences persistence in SQLite, session lifecycle and state tracking.
  - `worker.rs`: Subprocess lifecycle manager for `worker.mjs` over JSON-lines stdio protocol.
  - `safety.rs`: Navigation safety policy, origin scope confinement, blocked domain enforcement.
  - `types.rs`: Semantic locators, element targets, page snapshots, observation payloads.
  - `worker.mjs`: Provider-neutral browser worker supporting Firefox family (via native WebDriver BiDi) and Chromium family (Google Chrome, Chromium, Microsoft Edge, Brave, Vivaldi, Opera via Chrome DevTools Protocol over WebSocket) using Node 20+ built-ins.
- `project/`:
  - `discovery.rs`: Dynamic project root resolution, candidate ranking, bounded search traversal, exclusion filters (`node_modules`, `.git`, `dist`, `build`, etc.), OS Desktop path resolution via `dirs::desktop_dir()`.
  - `devserver.rs`: Development server process lifecycle, ready line detection, port extraction, framework recognition.
- `audit/`:
  - `workflow.rs`: Autonomous UI audit orchestrator: plan generation, server boot, multi-viewport responsive checks, route discovery, interaction exploration, finding aggregation, markdown report synthesis.
  - `routes.rs`: Static file and router config route extraction.
  - `findings.rs`: Categorized finding models (Bug, Accessibility, DesignInconsistency, Performance, Warning) and deduplication.
  - `report.rs`: Markdown report generator with summary metrics and reproducible evidence attachments.

---

## 3. Runtime Lifecycle & Dataflow

1. **User Action / Trigger**: Initiated via Desktop Chat UI, Floating HUD, Scheduled Routine, Global Command Palette (`Ctrl+K`), Voice Dictation, or Telegram command.
2. **Task Ingestion & Run Creation**: A unique run ID is assigned to an `AgentRun` entity, recording task input, executing agent profile, conversation reference, and model choice.
3. **Context Compilation**:
   - Relevant conversation messages retrieved.
   - Distinct-scope persistent memories injected (preferences, workspace facts, applications).
   - Agent team system prompt synthesized according to role.
4. **Model Iteration**:
   - Model router calls provider with chat history and tool definitions.
   - When vision is active, validated screenshots are encoded into `MessageContent::Parts` and attached to provider messages.
   - Provider returns thought content and tool calls.
5. **Centralized Authorization**:
   - Every tool call (shell, filesystem, computer action, MCP, skill, artifact) is converted into an `ExecutionRequest`.
   - The centralized policy engine evaluates shell mode, computer use mode, blocked domains, path boundaries, and credential sensitivity.
   - Actions requiring human confirmation generate persisted `ApprovalRequest` items.
6. **Tool Execution, Grounding & Artifact Registration**:
   - Authorized actions run within host, PTY, or sandbox.
   - Visual actions leverage Set-of-Marks grounding and post-action verification.
   - Generated files are registered in the durable `artifacts` database table.
   - Timeline engine records before-and-after visual diffs and target markers.
7. **Event Timeline & Persistence**:
   - Monotonic sequence numbers order all run events.
   - Tool outputs and assistant responses are persisted transactionally to SQLite in WAL mode.
8. **Completion**: Terminal states (`succeeded`, `failed`, `cancelled`, `interrupted`) are recorded, desktop notifications dispatched, and results reflected in UI.
