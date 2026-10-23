# Kyro v0.1.2 Release Notes

### ⚡ Kyro v0.1.2 — Critical Agent Runtime, Security, Worktree Isolation & Registry Remediation

Kyro is an open-source, local-first autonomous desktop AI agent built with **Tauri v2**, **Rust**, and **React 19**.

---

### 📦 Available Linux Distro Packages

Every major Linux distribution family is directly supported in this release:

| Distribution / OS | Package | Direct Download / Install Command |
| :--- | :---: | :--- |
| **Ubuntu / Debian / Linux Mint / Pop!_OS / Kali** | `.deb` | `sudo apt install ./Kyro_0.1.2_amd64.deb` |
| **Fedora / RHEL / CentOS / Rocky / Alma** | `.rpm` | `sudo dnf install ./Kyro-0.1.2-1.x86_64.rpm` |
| **openSUSE (Leap / Tumbleweed)** | `.rpm` | `sudo zypper install ./Kyro-0.1.2-1.x86_64.rpm` |
| **Arch Linux / Manjaro / EndeavourOS / SteamOS** | `.pkg.tar.zst` | `sudo pacman -U kyro-0.1.2-1-x86_64.pkg.tar.zst` |
| **Universal Linux (Zero Install, Standalone)** | `.AppImage` | `chmod +x Kyro_0.1.2_amd64.AppImage && ./Kyro_0.1.2_amd64.AppImage` |
| **Universal Portable Tarball (Custom Prefix)** | `.tar.gz` | `tar -xzf kyro-0.1.2-linux-x86_64.tar.gz && ./kyro-0.1.2-linux-x86_64/install.sh` |
| **Checksums & Integrity Verification** | `.txt` | `sha256sum -c SHA256SUMS.txt` |

---

### 🌟 What's New & Key Highlights in v0.1.2

#### 1. Agent Tool Permissions at Actual Execution Boundary
- **Centralized Enforcement**: Enforced agent profile-level tool restrictions directly within `execute_tool_governed` before execution, supplementing the existing `AuthorizationEngine`.
- **Tool Identifier Normalization**: Canonicalized tool names and resolved shell aliases (`sh`, `bash`, `terminal`, `shell` → `execute_shell`).
- **Forged Delegation Protection**: Rejects forged or hallucinated `delegate_task` calls immediately if the agent's profile has `delegation.enabled: false`.
- **Fail-Closed Resolution**: Unknown tool modes fail closed (`false`).

#### 2. Authoritative Workflow Worktree Containment & Reconciliation
- **Working Directory Propagation**: Propagated execution directory to `execute_shell` (`cmd_builder.current_dir(root)`) so shell commands and scripts run inside the assigned isolated workspace.
- **Fail-Closed Isolation**: Immediately halts execution if an isolated workspace was configured but does not exist on disk, preventing silent fallback to the host workspace.
- **Collision-Resistant Task Isolation**: Created unique worktree paths (`.kyro/worktrees/{workspace_id}_{task_id}_a{attempt}`) and Git branches (`kyro-wt-{workspace_id}-{task_id}-a{attempt}`) preventing collisions across parallel tasks and retries.
- **Base-Commit Conflict Detection**: Recorded `base_commit` at creation, computed `merge-base` against project `HEAD`, and detected concurrent modifications on touched files, marking `can_safely_merge: false` on conflicts.
- **Safe Reconciliation**: Added `apply_reconciliation` for safe non-fast-forward branch merging and diff application to the project root.

#### 3. Transactional Registry & Override Resilience
- **Prospective Pre-Write Validation**: Clones in-memory registry entries and validates cross-profile uniqueness (display names, aliases, IDs) prior to disk persistence.
- **Atomic Persistence**: Persists profile manifests using temporary files with atomic rename; rolls back in-memory state on prospective validation failure.
- **Built-in Override Diagnostics**: Preserves `override_error` diagnostics for invalid override manifests on disk while falling back gracefully to embedded defaults.

#### 4. Bounded Delegation & Scoped Memory Retrieval
- **Cycle-Free Delegation**: Detects and halts recursive delegation cycles by walking the SQLite database parent-run ancestor chain, and blocks self-delegation.
- **Fatal Child Run Persistence**: Halts delegation immediately if database run insertion fails.
- **Model Resolution**: Honors `model.mode` (`fixed` vs `inherit`).
- **Scoped Fact Retrieval**: Added `MemoryStore::build_context_block_scoped` to strictly filter facts by profile `context.memory_scopes` (`preference`, `agent`).

#### 5. Context-Stripped Mentions & UI Synchronization
- **Natural Language Mentions**: Added `strip_non_routable_text` and `extract_mention_tokens` in `AgentRegistry` to ignore mentions inside code fences, blockquotes, inline backticks, and email addresses.
- **Fallback Routing**: Routes turns to default `apex-orchestrator` with diagnostic reasons when requested agents are unavailable.
- **Teammate Roster Origin Badges**: Updated modal badges to "Built-in", "Override", and "Custom" in `TeammateRosterModal.tsx`.
- **Active Selection Fallback**: Automatically falls back to Apex and persists the selection if an active teammate is deleted or disabled.
- **Draft Retention**: Preserves form draft edits and surfaces validation diagnostics on backend save rejection.

#### 6. Verification & Quality Assurance
- **Rust Verification Suite**: 313 passing tests with 0 warnings (`cargo clippy --all-targets --all-features -- -D warnings`).
- **Frontend Test Suite**: 268 passing tests across 33 Vitest test files with &ge;95% coverage.

---

### 🛡️ Verification

Verify download integrity after downloading packages:

```bash
sha256sum -c SHA256SUMS.txt
```
