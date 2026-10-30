# Kyro Security Boundaries & Governance Architecture

## 1. Security Architecture Principles

1. **Zero Implicit Approval**: No tool call may proceed merely because a frontend flag (`autoApprove: true`) was sent. Security enforcement must occur exclusively at the backend Rust execution boundary.
2. **Centralized Policy Evaluation**: Every consequential operation (shell execution, filesystem read/write, mouse/keyboard manipulation, virtual display actions, MCP invocation, skill replay, container execution) passes through a unified policy engine.
3. **Fail-Closed Isolation**: When sandboxing is mandated or requested, unavailability of the sandbox runtime must abort execution rather than transparently falling back to host execution.
4. **Secrets Separation**: Credentials (API tokens, Telegram keys) are kept out of application settings tables, logs, error messages, and frontend state.
5. **Durable Approval Queue**: Human-in-the-loop approvals are first-class, race-safe, time-bounded entities associated with a specific run and action.

---

## 2. Policy Matrix & Execution Modes

| Subsystem | Configuration Mode | Behavior |
| :--- | :--- | :--- |
| **Shell** | `SafeAutoApprove` | Executes parsed read-only commands without side effects. Ambiguous commands, operators, pipes, substitutions require human approval or sandbox. |
| | `SandboxOnly` | Mandatory execution within isolated Docker/Podman container. If sandbox is absent, tool fails closed. |
| | `AskAlways` | Every shell invocation generates an approval request. |
| **Computer Use** | `FullAutonomous` | Mouse/keyboard actions proceed unless targeting blocked domains or sensitive credential checkpoints. |
| | `ConfirmDestructive` | Closing windows, terminal hotkeys, or bulk operations require approval. |
| | `AskAlways` | Every desktop input action requires explicit confirmation. |
| **Filesystem** | `WorkspaceConstrained` | Restricts reads and writes to canonical paths within allowed directory trees. Prevents directory traversal (`../`) and symlink escapes. |
| **MCP Tools** | `PrivilegedExternal` | MCP tools are treated as untrusted external code. Each call is classified dynamically (`classify_external_tool`), fingerprinted cryptographically, and inspected against governance settings. External modifying/execution tools cannot be auto-approved (`can_auto_approve: false`). |
| **Code Experiments** | `IsolatedSandbox` | Mandatory execution in ephemeral Docker/Podman container with read-only root, temporary tmpfs, dropped capabilities, no network by default, and strict CPU/memory quotas. Fails closed if container runtime is unavailable; never falls back to host. Pinned image allowlist enforced (`APPROVED_SANDBOX_IMAGES`). |
| **Deep Local RAG** | `LocalLoopbackOnly` | Retrieved context is untrusted user content and never changes authorization rules. Semantic embedding queries strictly require local loopback endpoints (`127.0.0.1`, `localhost`). External embedding calls are blocked to prevent cloud data exfiltration. Chunks strictly scoped to authorized folders. |
| **Habit Learning** | `PrivacyGatedOptIn` | Disabled by default. Only analyzes successful, completed runs. Passwords, secrets, tokens, API keys, private keys, and MFA checkpoints are strictly filtered out and never learned. Newly suggested routines are never auto-scheduled or executed without explicit human review (`enabled = false`). |

---

## 3. Approval Proof Binding & TOCTOU Defense

Human approvals are cryptographically and transactionally bound to the exact requested action:
1. **Binding Vectors**: Every approval proof is bound to:
   - Specific `approval_id`
   - Exact agent `run_id`
   - Exact `tool_name` (or canonical scoped tool name)
   - Exact execution arguments (`args_json`) compared using canonical JSON equivalence (`json_values_canonically_equal`).
2. **Atomic Consumption**: Consumption occurs inside an atomic SQLite transaction via `consume_approval_request_bound`, transitioning status from `approved` to `consumed`.
3. **Defense Against Replay & Tampering**: Any attempt to replay a consumed proof, substitute the tool name, alter the arguments, or present an expired approval is unconditionally rejected.

---

## 4. Human Takeover & Checkpoint Detection

When automated desktop control encounters sensitive authentication checkpoints:
- Two-Factor Authentication (2FA / OTP)
- Multi-Factor Authentication (MFA)
- CAPTCHA / Bot detection
- Password input fields
- Credit card / CVV / billing forms
- Blocked financial or banking domains (e.g. `chase.com`, `paypal.com`)

The policy engine automatically halts automation, records a high-risk approval/takeover request, and prompts the user for intervention. The model cannot bypass this boundary through prompt injection.

---

## 5. Credential Storage & Migration

- **Mechanism**: Dedicated operating system credential store (via OS Keyring / Secret Service / Windows Credential Manager / macOS Keychain), or authenticated platform encryption (AES-256-GCM with PBKDF2/Argon2 key derivation protected by machine/user identity when OS keyring daemon is unconfigured).
- **Migration**: Existing `secret:*` records in SQLite `settings` are read, imported into the secure store, verified, and purged from SQLite.
- **Redaction**: Credential values are sanitized from logs, IPC event payloads, and database messages.

---

## 6. Subprocess & Resource Limits

- Subprocesses spawned by shell execution or MCP transport have bounded timeouts, output byte limits (e.g., 512KB max output), and are monitored with kill-on-drop / asynchronous task cancellation tokens.
- Child process trees are tracked and terminated cleanly upon agent run cancellation or application exit.

---

## 7. Artifact & Template Export Security

- **Path Traversal Protection**: All artifact operations strictly enforce filename sanitization, rejecting directory traversal sequences (`..`), forward slashes (`/`), and backslashes (`\`). Files are stored exclusively in the designated `<app_data_dir>/artifacts/` directory.
- **Secret Redaction on Export**: Workspace template exports inspect all MCP environment variables and settings. Keys matching sensitive patterns (`KEY`, `TOKEN`, `SECRET`, `PASS`, `AUTH`) are automatically replaced with `<REDACTED_ON_EXPORT>`.
- **Validation on Import**: Incoming template packs validate schema version 1, confirm valid JSON structures, and verify safe alphanumeric identifiers before writing to SQLite.

---

## 8. Email Injection Defense & RFC 2822 Header Sanitization

- **Header Injection Scanner**: Outgoing email headers (`to`, `subject`, `in_reply_to`) are strictly inspected for `\r`, `\n`, `\0`, or ASCII control characters. Any attempted delimiter injection is rejected immediately before network dispatch.
- **Recipient Address Format**: Strictly validates single `@`, non-empty local parts, valid domain syntax, and RFC 5322 display-name formats (`User <user@example.com>`).
- **RFC 2047 Word Encoding**: Unicode subjects and non-ASCII display names are encoded into base64 MIME words (`=?UTF-8?B?...?=`) to preserve standards compliance across mail transfer agents.

---

## 9. POSIX TOCTOU Hardening & Filesystem Collision Safeguards

- **Immediate Re-Canonicalization**: Prior to moving, copying, or undoing filesystem operations, source and target paths are re-canonicalized and verified against authorized canonical workspace roots to prevent Time-of-Check to Time-of-Use (TOCTOU) symlink substitution attacks.
- **Destination Collision Guards**: Existing destination targets are inspected with `symlink_metadata()` to prevent writing through symlinks or clobbering existing files without explicit human confirmation (`confirm_destructive: true`).
- **Safe Pre-Removal**: When overwrite is explicitly confirmed, existing files are safely unlinked before recreation, preventing write-through symlink escapes.

---

## 10. Revoked-Folder Pruning & Fail-Closed Document Access

- **Atomic Pruning on Revocation**: When an authorized folder is removed via UI or API, `remove_authorized_folder_and_prune` executes an atomic SQLite transaction deleting the folder authorization and purging all indexed document records, cached text, and metadata located outside remaining authorized paths.
- **Startup Reconciliation**: App initialization actively audits all indexed documents against currently configured authorized folders, cleaning any stale records left by external changes.
- **Fail-Closed Access**: In-memory and IPC lookups (`get_indexed_document`) verify path confinement before returning document contents, returning `Ok(None)` if access is unauthorized.

---

## 11. OAuth Token Lifecycle & Single-Flight Synchronization

- **Single-Flight Refresh Mutex**: Concurrent tool invocations requiring refreshed OAuth tokens share an asynchronous mutex with a 15-second grace re-check, preventing duplicate token refresh requests against upstream providers.
- **Error Classification**: Distinguishes permanent authentication failures (`invalid_grant`, `invalid_client`, HTTP 4xx -> transitioning account status to `needs_reauth`) from transient network failures (DNS, timeout, HTTP 5xx -> transitioning account status to `error`).
- **Secret Scrubbing**: Client secrets and refresh tokens are scrubbed from user-facing error messages, application logs, and database records.

---

## 12. File Organization Anti-Tampering & Human Governance

- **Server-Authoritative Anti-Tampering**: Reversible file organization commands require an existing, pre-computed plan record in the SQLite `file_organization_plans` table. Ad-hoc or unsanctioned operations cannot execute.
- **Pre-Move Freshness & Hash Verification**: Before executing moves, source files undergo immediate SHA-256 hash checks. Any detected external modification or missing file invalidates the plan and halts execution.
- **Symlink Exclusion**: Symlinks are identified and excluded during scanning to avoid symlink traversal vulnerabilities and are reported as conflicts.
- **Mandatory Approval**: Mutating file reorganizations require explicit human-in-the-loop confirmation regardless of persisted permission level.
- **Durable Transactions**: Move operations are logged into `file_transactions` with pre- and post-hash states, enabling atomic one-click rollback (`undo_file_transaction`).

---

## 13. Attachment Policy & Binary Magic Byte Defense

- **Strict UTF-8 Validation**: Text attachments undergo character boundary validation to prevent broken encodings, truncation exploits, and prompt injection smuggling.
- **Binary Magic Byte Detection**: Attachments are inspected against known binary magic byte signatures (ELF `\x7fELF`, Mach-O, Windows PE `MZ`, ZIP `PK\x03\x04`, GZIP `\x1f\x8b`). Disguised executable files are rejected unconditionally.

---

## 14. Grant Provenance & Storage Lifecycle CAS Guards

- **Task Grant Provenance**: Granted permissions track originating task IDs and cannot be reused across sessions or hijacked by child processes.
- **Negation Priority**: Deny and negation directives always take priority over affirmative allowances.
- **Compare-And-Swap (CAS) Status Guards**: Agent run terminal states (`succeeded`, `failed`, `uncertain`, `cancelled`, `interrupted`) are immutable and protected against race conditions or late-arriving completion interleave.

---

## 15. Execution Trust, Outcome Verification & Direct IPC Normalization

- **Truthful Execution Lifecycle**:
  - The runtime strictly separates three independent operational facts:
    1. `action_dispatched`: the request to execute an action was sent to the target subsystem.
    2. `action_effect_observed`: immediate sensory or visual effects (such as screenshot pixel deltas or process spawn) were observed.
    3. `task_postcondition_verified`: the intended task postcondition was affirmatively and independently established on the host.
  - Run and task states transition across `pending`, `running`, `awaiting_approval`, `succeeded`, `failed`, `uncertain`, `unverified`, `cancelled`, and `interrupted`.
  - Stale callbacks, late tool events, or conversational completions cannot overwrite terminal states.
- **Affirmative Postcondition Verification**:
  - Verification success is never inferred from tool names (e.g., `verify_*`, `validate_*`), model descriptions, or arbitrary tool output strings.
  - Only typed `VerificationResult` instances emitted by Kyro's trusted backend execution layer can establish verified status.
  - File writes read back written bytes and cryptographically verify content or SHA-256 hashes on disk.
  - Shell commands check exit status and expected pattern matches.
  - Computer use actions monitor visual difference metrics; visual changes alone are observations (`action_effect_observed = true`), NOT proof of task completion (`task_postcondition_verified = false`). Actions without machine-readable application-state verification report `uncertain` or `not_applicable` rather than false success.
- **Absence of Success Fallbacks**:
  - Database outcome retrieval failures propagate as errors; they never fall back to `succeeded`.
  - Empty or incomplete event histories default to `unverified`, never `succeeded`.
  - Startup reconciliation reconciles active runs into `interrupted`, never `succeeded` or `pending`.
- **Non-Overridable Safeguards in FullAutonomous Mode**:
  - Destructive shortcut and key classification runs unconditionally before any general policy allow decision.
  - Destructive combinations (e.g., `Alt+F4`, `Ctrl+W`, `Ctrl+Q`, `Ctrl+Alt+Del`, `Cmd+Opt+Esc`, `Shift+Delete`, terminal interrupt signals) require human confirmation even when `FullAutonomous` mode is active.
  - Unknown computer actions, corrupt policies, and unrecognized policy modes fail closed.
- **Secret-Safe Approval Previews**:
  - Pending approval previews display normalized action targets (e.g. `Action: Hotkey (Alt+F4)`, working directories, file paths, and sanitized command strings).
  - Multi-tier recursive credential scrubbing automatically redacts Bearer tokens, JWT tokens, API keys (`sk_`, `sk-`, `ghp_`, `AKIA`, `sq0idp-`), private keys, database connection strings, basic auth URLs (`protocol://user:[REDACTED]@host`), passwords, and authorization headers from preview cards, logs, and audit records.
  - Keystroke and typed credential masking: Computer-use `type` and `key` actions targeting sensitive fields (passwords, PINs, secrets, tokens, credentials) replace raw text with `[REDACTED_CREDENTIAL]` without revealing character lengths or counts.
  - Actions are only labeled `Reversible` when an authoritative rollback path exists; unverified or destructive actions display `Unknown / Not guaranteed` or `Irreversible`.
- **Authoritative Consistency in Final Responses**:
  - Model-generated replies cannot override or conceal the authoritative task outcome.
  - Unverified tasks, failures, uncertainties, cancellations, and interruptions prepend or append authoritative execution notices to assistant messages in the UI and durable message store.
  - For `unverified` runs, the authoritative notice is appended unconditionally across all workflows, including single-turn requests, routine operations, and runs without a structured task plan.

---

## 16. Verification Binding Vector, Multi-Action Aggregation & Verification Test Suite

### Verification Binding Vector
Authoritative verification results must bind strictly to execution context:
1. `run_id`: Exact run identifier. Verification evidence for a different run is rejected.
2. `tool_call_id`: Exact tool call identifier. Verification evidence bound to an unmatched action ID is rejected.
3. `action_name`: Canonical tool/action identifier (e.g., `write_file`, `execute_shell`).
4. `expected_postcondition`: Explicit expected invariant (e.g., file existence, hash match, exit status).
5. `target_resource`: Canonical resource identifier (e.g., canonical path, PID).
6. Tool outputs claiming verification via untrusted fields (`trusted_verifier: true`, `task_postcondition_verified: true`, `verification.status: succeeded`) or tool names (`verify_*`, `test_*`, `run_tests`) without backend verification evidence are untrusted and discarded.

### Multi-Action Aggregation & Precedence Rules
Task completion evaluates every consequential action executed in the run:
- **Action-Level Tracking**: Each tool call ID is tracked to its authoritative outcome.
- **Outcome Precedence**:
  1. `failed`: If any consequential action failed, the whole run is `failed`.
  2. `uncertain`: If no action failed but any consequential action has uncertain outcome, the whole run is `uncertain`.
  3. `unverified`: If no action failed/uncertain but any consequential action lacks affirmative backend verification, the whole run is `unverified`. One verified action cannot compensate for another unverified action.
  4. `succeeded`: The whole run is `succeeded` only if ALL consequential actions achieved affirmative backend verification.
- **Read-Only Exemption Policy**: Purely read-only inspection operations (`read_file`, `list_directory`, `search_*`, `get_*`, `view_image`) do not require postcondition verification if no mutations occurred.
- **Terminal Immutability**: Runs in `cancelled` or `interrupted` states cannot transition to `succeeded` regardless of subsequent events.
- **Pending Approvals**: A run with unresolved approval requests at termination resolves to `awaiting_approval` or `unverified`, never `succeeded`.

### Running Verification & Desktop E2E Tests
```bash
# Whole-run verification aggregation and lifecycle tests (Rust backend)
cargo test --manifest-path libkyro/Cargo.toml --test whole_run_verification_tests

# Desktop E2E integration tests across Tauri mock runtime, DB, and state
cargo test --manifest-path libkyro/Cargo.toml --test desktop_e2e_tests

# Trust authorization and security unit/integration tests
cargo test --manifest-path libkyro/Cargo.toml --test trust_and_authorization_tests

# Frontend adversarial verification integrity and secret redaction tests
npm test kyroui/test/verification_integrity_and_redaction.test.tsx

# Frontend Desktop E2E scenarios across UI, TaskProgress, approvals, and Truthful reply
npm test kyroui/test/desktop_e2e_scenarios.test.tsx

# Full test suites
cargo test --manifest-path libkyro/Cargo.toml
npm test
```


