# Kyro v0.1.3 Release Notes

### ⚡ Kyro v0.1.3 — Modern Application Shell, Live Reasoning Streaming, Hardened File Organizer & Multi-Provider Isolation

Kyro is an open-source, local-first autonomous desktop AI agent built with **Tauri v2**, **Rust**, and **React 19**.

---

### 📦 Available Linux Distro Packages

Every major Linux distribution family is directly supported in this release:

| Distribution / OS | Package | Direct Download / Install Command |
| :--- | :---: | :--- |
| **Ubuntu / Debian / Linux Mint / Pop!_OS / Kali** | `.deb` | `sudo apt install ./Kyro_0.1.3_amd64.deb` |
| **Fedora / RHEL / CentOS / Rocky / Alma** | `.rpm` | `sudo dnf install ./Kyro-0.1.3-1.x86_64.rpm` |
| **openSUSE (Leap / Tumbleweed)** | `.rpm` | `sudo zypper install ./Kyro-0.1.3-1.x86_64.rpm` |
| **Arch Linux / Manjaro / EndeavourOS / SteamOS** | `.pkg.tar.zst` | `sudo pacman -U kyro-0.1.3-1-x86_64.pkg.tar.zst` |
| **Universal Linux (Zero Install, Standalone)** | `.AppImage` | `chmod +x Kyro_0.1.3_amd64.AppImage && ./Kyro_0.1.3_amd64.AppImage` |
| **Universal Portable Tarball (Custom Prefix)** | `.tar.gz` | `tar -xzf kyro-0.1.3-linux-x86_64.tar.gz && ./kyro-0.1.3-linux-x86_64/install.sh` |
| **Checksums & Integrity Verification** | `.txt` | `sha256sum -c SHA256SUMS.txt` |

---

### 🌟 What's New & Key Highlights in v0.1.3

#### 1. Modern Application Shell & Reusable Design System
- **Desktop Shell & Navigation**: High-density professional workspace layout with dedicated sidebar navigation, conversation switcher, quick action shortcuts, and real-time model status indicators.
- **Interactive Welcome Home Experience**: Zero-state home screen featuring one-click prompt suggestion chips for fast discovery of everyday workflows and developer tools.
- **Multiline Auto-Expanding Composer**: `TaskComposer` featuring multi-line auto-expansion, keyboard submission (Enter to send, Shift+Enter for newlines), drag-and-drop file/image previews, and `@mention` agent autocompletion.
- **Component System**: Modular UI library (`Button`, `Dialog`, `CodePanel`, `StatusBadge`, `Surface`, `TextField`) with keyboard focus trapping and ARIA compliance.
- **Per-Conversation Isolation**: Preserved active task plans and execution state across conversation switches without race conditions or memory leaks.

#### 2. Live Model Reasoning Streaming & Typography
- **SSE Stream Extraction**: Real-time extraction and streaming of model reasoning deltas and `<think>` blocks, strictly separated from final assistant responses and structured tool calls.
- **Interactive Reasoning Disclosures**: `ThinkingIndicator` component with dark-theme wave and shimmer animations, smooth auto-scrolling reasoning disclosures, honest waiting fallbacks, and `prefers-reduced-motion` compliance.
- **12px Fira Code Typography**: Consistent `@fontsource/fira-code` monospace font across code snippets, terminal views, and `ToolCallCard` cards.

#### 3. Hardened Safe Reversible File Organizer
- **Server-Authoritative Anti-Tampering**: Introduced `SharedFileOrganizer` with plan validation backed by SQLite `file_organization_plans`, ensuring only pre-computed, verified plans execute.
- **Pre/Post Move SHA-256 Freshness Verification**: Automatically aborts stale or externally modified operations by validating source file hashes immediately prior to moving.
- **Symlink Traversal Defense**: Detects and excludes symlinks during scanning, reporting them as conflicts to prevent link manipulation.
- **Authorized Folder Confinement**: Strictly confines file scanning and moving within authorized folder paths, handling `~` home directory paths safely.
- **Durable Undo Recovery**: Persists file transaction manifests in SQLite, enabling one-click atomic undo recovery (`undo_file_transaction`, `get_file_transaction`).
- **Human-in-the-Loop Governance**: Requires explicit user approval for mutating filesystem reorganizations.

#### 4. Model Routing & Provider Architecture
- **Native Anthropic Messages API**: Native Anthropic Messages API integration with system prompt formatting, streaming delta handling, and provider classification.
- **Sub-Route Isolation**: Isolated sub-routes and delegated tasks to prevent context pollution between parallel agent workflows.
- **Model-Specific Parameter Scoping**: Provider arguments (e.g., `chat_template_kwargs.enable_thinking`) scoped strictly to compatible models.

#### 5. Governance, Storage & Attachment Security
- **Storage CAS Status Guards**: Compare-And-Swap guards on run status updates in SQLite, making terminal outcomes (`succeeded`, `failed`, `cancelled`, `interrupted`) immutable against race conditions.
- **Grant Provenance & Negation Handling**: Strict tracking of task grant provenance to prevent privilege hijacking; robust handling of permission negation rules.
- **Connector Dynamic Policy Enforcement**: Prevents external dynamic tools from self-authorizing through naming heuristics; mandates explicit local policy configuration.
- **Strict Attachment Validation**: Enforces UTF-8 validation on text attachments and rejects binary executable magic bytes (ELF, Mach-O, PE, ZIP, GZIP).
- **Bounded OAuth Polling**: Bounded background OAuth authentication polling attempts with automatic timer cancellation and cleanup on component unmount.

#### 6. Verification & Quality Assurance
- **Rust Verification Suite**: 317 passing tests across unit, integration, and security suites with 0 warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- **Frontend Test Suite**: 318 passing tests across 39 Vitest suites with &ge;95% code coverage.

---

### 🛡️ Verification

Verify download integrity after downloading packages:

```bash
sha256sum -c SHA256SUMS.txt
```
