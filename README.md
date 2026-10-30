<p align="center">
  <img src="logo.svg" alt="Kyro Logo" width="128" height="128" />
</p>

<h1 align="center">Kyro</h1>

<p align="center">
  <strong>Local-First Autonomous Desktop AI Agent</strong><br />
  Built with Tauri v2, Rust & React 19.
</p>

<p align="center">
</p>

<p align="center">
  <a href="https://launchbuff.com/products/kyro-npnc71" target="_blank" rel="noopener noreferrer" title="Featured on LaunchBuff"><img src="https://launchbuff.com/badge-featured-dark.svg" alt="Featured on LaunchBuff" width="256" height="80" /></a>
</p>

<p align="center">
  <a href="https://github.com/eshanized/Kyro/actions/workflows/ci.yml"><img src="https://github.com/eshanized/Kyro/actions/workflows/ci.yml/badge.svg" alt="Continuous Integration" /></a>
  <a href="https://github.com/eshanized/Kyro/discussions"><img src="https://img.shields.io/badge/Discussions-Join-purple?logo=github&logoColor=white" alt="GitHub Discussions" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
  <a href="https://tauri.app"><img src="https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri&logoColor=white" alt="Tauri v2" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.75+-DEA584?logo=rust&logoColor=white" alt="Rust 1.75+" /></a>
  <a href="https://react.dev"><img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19" /></a>
  <a href="https://www.typescriptlang.org/"><img src="https://img.shields.io/badge/TypeScript-5.7-3178C6?logo=typescript&logoColor=white" alt="TypeScript 5.7" /></a>
  <a href="#-zero-trust-security--privacy"><img src="https://img.shields.io/badge/Privacy-100%25%20Local--First-2ea44f?logo=sqlite&logoColor=white" alt="Local-First Privacy" /></a>
  <a href="#-architecture--demo-walkthrough"><img src="https://img.shields.io/badge/Demo-Architecture%20Walkthrough-FF0000?logo=github&logoColor=white" alt="Demo Walkthrough" /></a>
  <a href="CONTRIBUTING.md"><img src="https://img.shields.io/badge/PRs-welcome-brightgreen.svg" alt="PRs Welcome" /></a>
</p>

---

## 🌟 Overview

**Kyro** is a high-performance, local-first autonomous desktop AI agent that bridges natural language reasoning with real computer interaction. Unlike web-wrapped wrappers or cloud-dependent bots, Kyro runs a multi-threaded Rust native core with Tauri v2 and React 19, delivering zero-latency screen understanding, browser automation, terminal execution, and secure Model Context Protocol (MCP) integrations.

Kyro is built with a **Zero-Trust Security Model**: all conversations, embeddings, task histories, and artifacts stay strictly on your local disk in SQLite (WAL mode). API keys and credentials are encrypted using your Operating System Keyring (`Vault`), and dangerous actions require argument-aware authorization and human-in-the-loop verification.

---

## 🎬 Architecture & Demo Walkthrough

<div align="center">
  <img src="assets/Architecting_the_Autonomous_Workspace__Deconstructing_Kyro_s_Ze-cleaned.gif" alt="Architecting the Autonomous Workspace: Deconstructing Kyro's Zero-Trust Architecture" width="100%" />
  <p><em>Architecting the Autonomous Workspace: Deconstructing Kyro's Zero-Trust Architecture & Autonomous Workflow.</em></p>
  <p><a href="assets/Architecting_the_Autonomous_Workspace__Deconstructing_Kyro_s_Ze-cleaned.gif"><strong>🔍 View Full-Resolution Animated Walkthrough (.gif)</strong></a> &bull; <a href="assets/Kyro.mp4"><strong>▶️ Download Full Audio Video Demo (.mp4)</strong></a></p>
</div>

<br />

### 📖 Architecture Breakdown & Deep-Dive Explanation

The animated walkthrough (`Architecting the Autonomous Workspace: Deconstructing Kyro's Zero-Trust Architecture`) provides a visual step-by-step breakdown of Kyro's architecture, demonstrating how it overcomes the critical failure modes of conventional LLM agents through local-first execution, zero-trust security boundaries, and affirmative verification:

1. **The Reliability Paradox: Why Unchecked LLMs Fail on Real Desktops (00:00 – 01:30)**
   - **The Problem**: Conventional AI agents treat desktop automation as open-ended text generation. When unprivileged models emit commands directly to a host, they frequently hallucinate non-existent arguments, misinterpret syntax, and cause cascading system errors (such as unhandled null-pointer exceptions or terminal crashes) because they operate without a deterministic ground-truth feedback loop.
   - **Kyro's Approach**: Replaces open-ended text guessing with typed, stateful task execution plans. The runtime separates three distinct operational facts: *action dispatched*, *sensory effect observed*, and *affirmative postcondition verified*. A task cannot report completion without machine-verifiable evidence.

2. **Centralized Zero-Trust Engine & Lexical Shell Firewall (01:30 – 03:30)**
   - **The Threat**: Prompt injection, malicious workspace repos, or runaway model loops attempting destructive operations (e.g., `cat server.log | rm -rf /`).
   - **Lexical Analysis**: Kyro's Rust security kernel intercepts all raw shell inputs before they touch the host OS. Commands are lexically parsed into structured `argv[]` token vectors (`argv[0] = "cat"`, `argv[1] = "server.log"`, `argv[2] = "|"`, `argv[3] = "rm"`, `argv[4] = "-rf"`).
   - **Injection Firewall**: Shell metacharacters, subshells, pipelines, and chained commands are identified immediately. Any unverified shell sequence fails closed, halting execution before any command is spawned.

3. **Ephemeral Container Sandboxing & Least Privilege (03:30 – 04:30)**
   - **Execution Isolation**: When untrusted scripts or experimental code runs, Kyro delegates execution to an isolated container runtime (`docker run --network none --security-opt no-new-privileges --cap-drop ALL`).
   - **Zero Host Risk**: All Linux capabilities are dropped, outbound network connections are blocked by default (`NO_NET`), the root filesystem is mounted read-only, and memory/CPU quotas prevent fork-bombs and host contamination. If the container engine is missing, Kyro fails closed rather than silently falling back to the host.

4. **Developer Workbench & Intelligent Diagnostic Triage (04:30 – 05:30)**
   - **Build Doctor**: Intercepts compiler errors (such as Rust `cargo build --release` compiler errors like `E0308`), maps the offending source file and span, and provides automated diagnostic triage.
   - **UI Audit & Visual Regression**: Captures multi-viewport snapshots and compares layout geometry across viewports, detecting microscopic visual flaws (such as button coordinate shifts `(x+5, y+2)`) before frontend bugs reach production.

5. **Human Takeover & Cryptographically Bound Approvals (05:30 – 06:15)**
   - **Sensitive Checkpoints**: When automation encounters credential forms, 2FA/MFA challenges, CAPTCHAs, or destructive system shortcuts (e.g., `Alt+F4`, `Ctrl+W`, `Shift+Delete`), automation immediately pauses for human intervention.
   - **Anti-TOCTOU & Proof Binding**: Human approvals emit a single-use authorization token cryptographically bound to the exact `run_id`, canonical `action`, and SHA-256 hash of the execution arguments. Replay attacks, argument tampering, or cross-run token reuse are strictly rejected.

6. **Hardware-Encrypted OS Keyring & Ephemeral Secret Lifecycle (06:15 – 06:45)**
   - **Zero Plaintext Persistence**: Public settings stay in SQLite WAL on disk, while API keys and OAuth tokens reside exclusively in the OS Keyring (macOS Keychain, Linux Secret Service, Windows Credential Manager).
   - **Ephemeral In-Memory Tokens**: Tokens are loaded into volatile memory only for the exact duration of an authenticated network call and immediately dropped and zeroed out (`token_exposed: false`).
   - **Scrubbing Engine**: Multi-tier redaction automatically sanitizes Bearer tokens, private keys, JWTs, and passwords from UI cards, logs, and database records.

7. **Native MCP Host & Multi-Agent Collaboration (06:45 – 07:25)**
   - **Model Context Protocol (MCP)**: Dynamic tool discovery and execution via JSON-RPC 2.0 (stdio and HTTP/SSE) connects Kyro to standard MCP servers (e.g., `@modelcontextprotocol/server-filesystem`) with fine-grained per-tool permissions.
   - **Multi-Agent Workspace**: Autonomous specialist agents (**Apex**, **Forge**, and **Scout**) operate on independent Git worktrees, allowing parallel development, testing, and auditing without workspace conflicts or state contamination.

---

## 📸 Visual Showcase

<div align="center">
  <h3>Everyday Mode & Interactive Assistant</h3>
  <img src="assets/screenshot_1.png" alt="Kyro Everyday Mode" width="90%" />
</div>

<br />

<div align="center">
  <h3>Developer Workbench & Managed Dev Servers</h3>
  <img src="assets/screenshot_2.png" alt="Kyro Dev Workbench" width="90%" />
</div>

<br />

<details>
<summary><strong>🔍 Click to view more features and interface previews</strong></summary>
<br />

| Safe Reversible File Organizer | Local-First Document Assistant |
|:---:|:---:|
| <img src="assets/screenshot_3.png" alt="File Organizer" width="100%" /> | <img src="assets/screenshot_4.png" alt="Document Assistant" width="100%" /> |
| *Categorization rules & SHA-256 duplicate detection* | *Text/PDF parsing, date extraction, and side-by-side diffing* |

| Multimodal Screen Explainer | Teach-a-Task & Routine Scheduler |
|:---:|:---:|
| <img src="assets/screenshot_5.png" alt="Screen Explainer" width="100%" /> | <img src="assets/screenshot_6.png" alt="Teach a Task" width="100%" /> |
| *Visual fact extraction, inferences, and 1-click actions* | *Demonstration recorder with cron & interval scheduling* |

| Connections & MCP Integration Center | Multi-Agent Worktree Workspace |
|:---:|:---:|
| <img src="assets/screenshot_7.png" alt="Connections & MCP" width="100%" /> | <img src="assets/screenshot_8.png" alt="Multi-Agent Workspace" width="100%" /> |
| *Model Context Protocol, OAuth PKCE, and token isolation* | *Specialist agents (Apex, Forge, Scout) with isolated git worktrees* |

</details>

---

## ✨ Key Features

### 🖥️ Dual Experience Modes
- **Everyday Mode**: Intuitive natural language workflows for non-developers:
  - **File Organizer**: Reversible file organization, collision handling, and SHA-256 deduplication.
  - **Document Assistant**: In-depth local PDF and plaintext parser with date/action item extraction and privacy disclosures.
  - **Screen Explainer**: Multimodal visual grounding with grounded 1-click prompt triggers.
- **Dev Workbench**: Specialized tools for software engineers:
  - **Projects & Dev Servers**: Auto-discovery of local projects, port status detection, and managed process lifecycles.
  - **Code Checkpoints & Diffs**: Instant git-aware snapshots, working tree clean checks, and rollback safety.
  - **Build Doctor**: Automated diagnostic triage for build failures, dependency issues, and syntax errors.
  - **UI Audit & Visual Regression**: Visual defect detection with multi-viewport baselines (Desktop, Tablet, Mobile).
  - **Multi-Agent Workspace**: Isolated Git worktree execution for autonomous teammates (**Apex**, **Forge**, **Scout**).

### 🔒 Zero-Trust Security & Privacy
- **100% Local-First Storage**: SQLite with WAL mode stores chat histories, memories, and task traces on your machine.
- **OS Keyring Vault**: API keys and OAuth tokens are stored in the OS credential store (macOS Keychain, Linux Secret Service, Windows Credential Manager) with AES-256-GCM authenticated fallback.
- **Argument-Aware Shell Governance**: Shell commands are lexically parsed into exact `argv` tokens; command injection and unverified metacharacters are rejected.
- **Human-in-the-Loop Takeover**: Automatic detection of sensitive checkpoints (2FA prompts, password inputs, CAPTCHA) with smooth interactive user takeover.
- **Secret Redaction**: Exported routines, templates, and IPC event payloads automatically redact sensitive credentials.

### 👁️ Computer-Use & Visual Grounding
- **Set-of-Marks (SoM) Grounding**: Precise visual tagging and coordinate targeting.
- **Native Input Synthesis**: Low-level mouse and keyboard emulation via `enigo`.
- **Browser Automation**: High-fidelity, provider-neutral browser automation supporting Firefox (native WebDriver BiDi) and Chromium-family browsers (Google Chrome, Chromium, Brave, Microsoft Edge, Vivaldi, Opera via CDP) with origin-scoping and strict non-substitution hierarchy.
- **Interactive Heads-Up Display (HUD)**: Compact floating snipping window (`Ctrl+K` instant search, region selector).

### 🧩 Connectors & Model Context Protocol (MCP)
- Full client implementation of the **Model Context Protocol (MCP)** (stdio, HTTP JSON-RPC 2.0, and SSE transports).
- Native **GitHub Connector** and **Google Connector** (Gmail & Google Calendar) with secure PKCE OAuth 2.0, single-flight token refresh, RFC 2822 header injection defenses, and RFC 2047 MIME encoding.
- Support for custom local stdio tools and remote SSE/HTTP MCP endpoints.
- Per-action governance policy management (Allow, Ask Always, Deny).

### ⏰ Teach-a-Task & Remote Notification Bridge
- Record user demonstrations and convert them into reusable, parameterized skills.
- Cron and interval scheduler with timezone awareness and concurrency overlap guards.
- **Telegram Bridge**: Secure bidirectional notifications and mobile task triggering.

---

## 🏗️ System Architecture

```mermaid
graph TD
    UI["React 19 / Vite UI (Desktop + HUD + Overlay + Command Palette)"]
    Tauri["Tauri 2 IPC Bridge (Typed Commands & Events)"]
    Engine["AgentLoopEngine (Multi-step Task Orchestration)"]
    Auth["Centralized Security & Authorization Engine"]
    Router["Model Provider Router (NVIDIA NIM / Multimodal / Local)"]
    CU["Computer-Use Subsystem (xcap, enigo, SoM Grounding)"]
    PTY["Terminal / PTY Session Manager (portable-pty)"]
    Sandbox["Container / Sandbox Isolation"]
    MCP["Model Context Protocol (MCP) Client Manager"]
    Conn["ConnectorManager (Native GitHub, Remote MCP, Local MCP)"]
    DB["SQLite Storage & Versioned Migrations (WAL Mode)"]
    Vault["OS Keyring / Authenticated Encrypted Vault Service"]
    Routine["Routine Scheduler (Cron / Timezones)"]
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

For in-depth architectural specifications:
- [Runtime Overview Specification](docs/architecture/runtime-overview.md)
- [Security Boundaries & Threat Model](docs/architecture/security-boundaries.md)
- [Connector System & Remote MCP Architecture](docs/architecture/connector-system.md)
- [Feature Implementation Matrix](docs/implementation/feature-matrix.md)

---

## ⚡ Quick Install & Uninstall (via curl)

Install the latest release of Kyro directly into `~/.local/bin` (no root required) or `/usr/local/bin` (if run as root):

```bash
curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/install.sh | bash
```

Custom installation directory (e.g. `/usr/local`):
```bash
curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/install.sh | bash -s -- --prefix /usr/local
```

To uninstall Kyro anytime:
```bash
curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/uninstall.sh | bash
```

To purge all local configuration, databases, and checkpoints upon uninstallation:
```bash
curl -fsSL https://raw.githubusercontent.com/eshanized/Kyro/master/uninstall.sh | bash -s -- --purge
```

---

## 📥 Prebuilt Linux Distribution Packages

Prebuilt packages for every major Linux distribution family are published under each [GitHub Release](https://github.com/eshanized/Kyro/releases):

| Distribution / Environment | Package Format | Quick Install Command |
|:---|:---:|:---|
| **Ubuntu, Debian, Pop!_OS, Linux Mint, Kali** | `.deb` | `sudo apt install ./Kyro_0.1.4_amd64.deb` |
| **Fedora, RHEL, CentOS Stream, Rocky, Alma** | `.rpm` | `sudo dnf install ./Kyro-0.1.4-1.x86_64.rpm` |
| **openSUSE (Leap / Tumbleweed)** | `.rpm` | `sudo zypper install ./Kyro-0.1.4-1.x86_64.rpm` |
| **Arch Linux, Manjaro, EndeavourOS, SteamOS** | `.pkg.tar.zst` | `sudo pacman -U kyro-0.1.4-1-x86_64.pkg.tar.zst` |
| **Universal Linux (Standalone / Zero Install)** | `.AppImage` | `chmod +x Kyro_0.1.4_amd64.AppImage && ./Kyro_0.1.4_amd64.AppImage` |
| **Portable Linux Archive (Custom Prefix)** | `.tar.gz` | `tar -xzf kyro-0.1.4-linux-x86_64.tar.gz && ./kyro-0.1.4-linux-x86_64/install.sh` |

Verify asset integrity with `sha256sum -c SHA256SUMS.txt`.

---

## 🚀 Building from Source

### 1. Prerequisites

Ensure your development environment meets the following requirements:

- **Node.js**: `>= 20.x` and `npm`
- **Rust**: `>= 1.75.0` (`cargo`, `rustc`)
- **System Dependencies**:
  - **Ubuntu / Debian**:
    ```bash
    sudo apt-get update && sudo apt-get install -y \
      libwebkit2gtk-4.1-dev \
      build-essential \
      curl \
      wget \
      file \
      libxdo-dev \
      libssl-dev \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      xvfb
    ```
  - **macOS**:
    ```bash
    xcode-select --install
    ```
  - **Windows**:
    - [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
    - [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

### 2. Installation

Clone the repository and install Node dependencies:

```bash
git clone https://github.com/eshanized/Kyro.git
cd Kyro
npm install
```

### 3. Running in Development

Kyro includes a complete Makefile for common workflows:

```bash
# Run full desktop app (Vite frontend + Rust native backend)
make dev

# Run Vite frontend in browser (http://localhost:1420)
make dev-ui

# Run full desktop app with verbose debug logging
make dev-debug

# Run headless under virtual X11 framebuffer (Linux / CI)
make dev-headless
```

### 4. Configuration

Configure model credentials in **Settings** within the Kyro desktop application, or copy `.env.example`:

```bash
cp .env.example .env
```

Keys configured in UI Settings are encrypted inside your OS Keyring and will never be committed or written in plaintext.

---

## 🛠️ Verification & Testing

Kyro has strict test coverage across unit, integration, and security layers (over 346 backend Rust tests and 365 frontend Vitest tests across 43 test suites, achieving &ge; 95% frontend coverage and 711+ total automated tests):

```bash
# Run the complete end-to-end verification pipeline:
make verify
```

This single command runs:
1. `make fmt-check` — Rust code formatting validation (`rustfmt`)
2. `make clippy` — Rust Clippy linter with zero warnings (`-D warnings`)
3. `make test` — All Rust unit, integration, and security regression tests
4. `make test-ui` — Frontend Vitest suite and TypeScript compiler check (`tsc --noEmit`)
5. `make build-ui` — Production frontend bundle build

---

## 📦 Building for Production

To build optimized release binaries and desktop bundles:

```bash
# Build production frontend and release desktop binary
make build

# Build platform installers (.deb, .AppImage, .dmg, .msi)
make tauri-build
# or: npm run tauri build
```

Installers will be generated in `libkyro/target/release/bundle/`.

---

## 📁 Repository Structure

```
Kyro/
├── assets/                  # High-resolution screenshots and brand assets
│   ├── logo.svg
│   └── screenshot_*.png
├── docs/                    # Architectural and implementation specifications
│   ├── architecture/        # Runtime, security, and connector specifications
│   └── implementation/      # Feature matrices and remediation records
├── kyroui/                  # React 19 + TypeScript + Tailwind frontend
│   ├── components/          # Modals (Workbench, Organizer, Assistant, Explainer, etc.)
│   ├── hud/                 # Floating HUD and snipper overlays
│   ├── test/                # Vitest frontend test suites
│   ├── types/               # TypeScript domain models and IPC contracts
│   └── App.tsx              # Main dual-mode application shell
├── libkyro/                 # Native Rust backend (Tauri v2)
│   ├── src/                 # Engine, governance, computer_use, connectors, storage
│   ├── tests/               # Integration, security, and UI audit test suites
│   ├── tauri.conf.json      # Tauri v2 desktop window & permissions configuration
│   └── Cargo.toml           # Rust package manifest
├── .github/                 # GitHub Actions CI/CD workflows and issue templates
│   ├── workflows/           # CI and Release multi-platform workflows
│   ├── ISSUE_TEMPLATE/      # Bug report and feature request templates
│   └── PULL_REQUEST_TEMPLATE.md
├── Makefile                 # Unified development, testing, and build automation
├── package.json             # Frontend dependencies and scripts
├── tailwind.config.js       # Tailwind CSS design system configuration
└── vite.config.ts           # Vite build and bundling configuration
```

---

## 🤝 Contributing

Contributions are welcome and appreciated! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for step-by-step setup instructions, commit conventions, and pull request guidelines.

Please review our [Code of Conduct](CODE_OF_CONDUCT.md) before participating.

---

## 🛡️ Security

Security is foundational to Kyro. If you discover a vulnerability or security flaw, please review our [Security Policy](SECURITY.md) and report it privately to [m.eshanized@gmail.com](mailto:m.eshanized@gmail.com) rather than opening a public issue.

---

## 📄 License

Kyro is licensed under the [MIT License](LICENSE).
Copyright &copy; 2026 Eshan Roy and Kyro Contributors.
