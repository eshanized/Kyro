# Kyro v0.1.4 Release Notes

### ⚡ Kyro v0.1.4 — Authoritative Verification Contracts, Truthful Execution Lifecycle, Deep Credential Redaction & Specialized Multi-Agent Personas

Kyro is an open-source, local-first autonomous desktop AI agent built with **Tauri v2**, **Rust**, and **React 19**.

---

### 📦 Available Linux Distro Packages

Every major Linux distribution family is directly supported in this release:

| Distribution / OS | Package | Direct Download / Install Command |
| :--- | :---: | :--- |
| **Ubuntu / Debian / Linux Mint / Pop!_OS / Kali** | `.deb` | `sudo apt install ./Kyro_0.1.4_amd64.deb` |
| **Fedora / RHEL / CentOS / Rocky / Alma** | `.rpm` | `sudo dnf install ./Kyro-0.1.4-1.x86_64.rpm` |
| **openSUSE (Leap / Tumbleweed)** | `.rpm` | `sudo zypper install ./Kyro-0.1.4-1.x86_64.rpm` |
| **Arch Linux / Manjaro / EndeavourOS / SteamOS** | `.pkg.tar.zst` | `sudo pacman -U kyro-0.1.4-1-x86_64.pkg.tar.zst` |
| **Universal Linux (Zero Install, Standalone)** | `.AppImage` | `chmod +x Kyro_0.1.4_amd64.AppImage && ./Kyro_0.1.4_amd64.AppImage` |
| **Universal Portable Tarball (Custom Prefix)** | `.tar.gz` | `tar -xzf kyro-0.1.4-linux-x86_64.tar.gz && ./kyro-0.1.4-linux-x86_64/install.sh` |
| **Checksums & Integrity Verification** | `.txt` | `sha256sum -c SHA256SUMS.txt` |

---

### 🌟 What's New & Key Highlights in v0.1.4

#### 1. Authoritative Verification Contracts & Operational Truth (#36)
- **Elimination of Heuristic & Untrusted Claims**: Completely removed name-based verification heuristics (`verify_*`, `test_*`, `npm test`) and discarded self-attesting tool output JSON fields (`trusted_verifier`, `task_postcondition_verified`).
- **Strict Verification Binding**: Verification records strictly bind `run_id`, `tool_call_id`, `action_name`, expected postcondition, and target resource; cross-run or replay attacks are rejected.
- **Typed Operational Fact Separation**: Enforced rigorous separation between `action_dispatched`, `action_effect_observed`, and `task_postcondition_verified`.
- **Whole-Run Multi-Action Outcome Aggregation**: Rewrote run outcome aggregation with strict precedence (`failed` > `uncertain` > `unverified` > `succeeded`). A verified action can never mask an unverified or failed consequential action.
- **`unverified` Status Persistence**: Added `"unverified"` to SQLite `VALID_RUN_STATUSES`, allowing truthful unverified outcomes to persist safely without database rejections.
- **Truthful Assistant Messaging**: Appended truthful unverified warnings across all workflows (including single-turn requests) whenever runs terminate without authoritative verification.
- **Terminal Immutability**: Guaranteed terminal run states (`cancelled`, `interrupted`, `failed`) and blocked runs with pending approvals from resolving to `succeeded`.

#### 2. Deep Credential Scrubbing & Keystroke Masking
- **Secret-Safe Approval Previews**: Built deep regex credential redaction covering JWTs, Bearer tokens, GitHub PATs (`ghp_`), OpenAI API keys (`sk_`), Anthropic API keys (`sk-ant-`), AWS access keys (`AKIA`), Square tokens, shell exports, and basic auth URLs.
- **Computer-Use Keystroke Masking**: Sensitive targets (password fields, authentication dialogs) mask input strokes and typing payloads as `[REDACTED_CREDENTIAL]` without leaking character counts or keystrokes.
- **Non-Overridable Destructive Safeguards**: Preserved destructive desktop hotkey safeguards even in Full Autonomous mode to prevent destructive system-level interrupts.
- **Single-Use Approval Consumption**: Protected approval requests against replay and double-execution attacks.

#### 3. Specialized Multi-Agent Swarm Personas
- **Specialized Personas**: Introduced built-in specialist profiles for Code Review (`code-reviewer`), Documentation Review (`doc-reviewer`), and Quality Assurance (`test-engineer`).
- **Child Delegation API Key Scoping**: Added safe caller API key fallback for child delegations when vault keys are unset, avoiding delegation breakage while maintaining provider isolation (#23).
- **Fixed-Model Selection**: Honored fixed-model selection for primary agent runs (#24).

#### 4. Multi-Agent Worktree Reconciliation & Workspace DAG
- **Terminal DAG Candidate Resolution**: Reconciled the final integrated multi-agent worktree via terminal DAG candidate resolution (#28).
- **Explicit Apply Action**: Routed multi-agent reconciliation through an explicit user-controlled apply action (#27).
- **Non-Git Shadow-Copy Conflict Safety**: Hardened non-Git shadow-copy reconciliation with hash and timestamp checks to prevent silent file overwrites (#29).
- **Real Visual Regression Baselines**: Captured real visual regression baseline screenshots during UI inspection (#30).
- **Skill Replay Progress Streaming**: Streamed Teach-a-Task replay progress via real-time `skill:progress` events (#31).
- **Truthful Artifact Feedback**: Made artifact feedback delivery and persistence truthful with error propagation (#32).
- **Dev Workbench Enhancements**: Formatted millisecond timestamps accurately (#33) and aligned Dev Workbench experiment controls with backend APIs (#26).
- **Credential Vault Provider Coverage**: Expanded Settings and credential vault coverage across all router-enabled model providers (#34).
- **Truthful Empty Model Response Handling**: Handled empty model responses truthfully without falsely reporting completion (#35).
- **Today Dashboard Approval Alignment**: Fixed approval command payload structure in the Today dashboard (#25).

#### 5. Hardened Safe File Organization Tool
- **Detailed Descriptions & Exclusion Rules**: Enhanced file organization tools with fine-grained pattern exclusions, ignore rules, and descriptive category labels.
- **Safe Transaction Journaling**: Maintained SQLite transaction manifests for atomic preview and one-click undo recovery.

#### 6. Architecture Walkthrough & Documentation
- **Architecture Walkthrough Video & Animation**: Added visual architecture walkthrough animation and demo video to the repository with Git-LFS tracking.
- **Comprehensive README Guide**: Documented autonomous workspace architecture, zero-trust security boundaries, and multi-agent lifecycle.

#### 7. Verification & Quality Assurance
- **Frontend Test Suite**: 365 passing tests across 43 Vitest suites (&ge;95% code coverage).
- **Rust Verification Suite**: Over 320 passing unit, integration, and security tests with 0 warnings.
- **Comprehensive E2E Suites**: Added backend Desktop E2E suite (`desktop_e2e_tests.rs`) and frontend E2E scenarios (`desktop_e2e_scenarios.test.tsx`, `verification_integrity_and_redaction.test.tsx`) covering Scenarios A through J.

---

### 🛡️ Verification

Verify download integrity after downloading packages:

```bash
sha256sum -c SHA256SUMS.txt
```
