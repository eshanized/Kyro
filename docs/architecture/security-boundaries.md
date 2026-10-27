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

- **Truthful Execution Lifecycle**: Operations distinguish action execution from outcome postcondition verification. Run and task states transition across `pending`, `running`, `awaiting_approval`, `succeeded`, `failed`, `uncertain`, and `cancelled`.
- **Postcondition Verification**:
  - File writes read back target bytes and verify SHA-256 cryptographic hashes before confirming success.
  - Shell commands check exit status and expected output tokens; nonzero exits report explicit failure.
  - Computer use actions monitor visual difference metrics; actions producing negligible screen delta (e.g., `<= 5%`) report `uncertain` rather than false success.
- **Safe-by-Default Computer Use**:
  - New installations default to `ConfirmDestructive` instead of `FullAutonomous`.
  - Unrecognized or corrupted policy values fail closed to `AskAlways` / `ConfirmDestructive`.
  - Settings migrations preserve returning users' explicit configurations via `CURRENT_POLICY_SCHEMA_VERSION = 1`.
- **Canonical Action Normalization & Hotkey Classification**:
  - Direct IPC calls (`perform_computer_action`) and agent tool calls route through a single canonical `NormalizedComputerAction` parser.
  - Multi-modifier shortcuts (`Ctrl`, `Alt`, `Shift`, `Super`/`Meta`) across Linux, macOS, and Windows are classified truthfully against destructive impact tables.
- **Single-Use Approval Consumption**:
  - Pending approval requests are consumed atomically inside a single SQLite transaction with strict parameter and run binding (`consume_approval_request_bound`). Replay attacks and parameter alterations fail closed.
  - User-facing approval dialogs provide transparent inspections with automatic redaction of sensitive credentials (`token`, `password`, `secret`, `api_key`).

