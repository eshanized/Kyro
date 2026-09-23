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

## 4. Credential Storage & Migration

- **Mechanism**: Dedicated operating system credential store (via OS Keyring / Secret Service / Windows Credential Manager / macOS Keychain), or authenticated platform encryption (AES-256-GCM with PBKDF2/Argon2 key derivation protected by machine/user identity when OS keyring daemon is unconfigured).
- **Migration**: Existing `secret:*` records in SQLite `settings` are read, imported into the secure store, verified, and purged from SQLite.
- **Redaction**: Credential values are sanitized from logs, IPC event payloads, and database messages.

---

## 5. Subprocess & Resource Limits

- Subprocesses spawned by shell execution or MCP transport have bounded timeouts, output byte limits (e.g., 512KB max output), and are monitored with kill-on-drop / asynchronous task cancellation tokens.
- Child process trees are tracked and terminated cleanly upon agent run cancellation or application exit.

---

## 6. Artifact & Template Export Security

- **Path Traversal Protection**: All artifact operations strictly enforce filename sanitization, rejecting directory traversal sequences (`..`), forward slashes (`/`), and backslashes (`\`). Files are stored exclusively in the designated `<app_data_dir>/artifacts/` directory.
- **Secret Redaction on Export**: Workspace template exports inspect all MCP environment variables and settings. Keys matching sensitive patterns (`KEY`, `TOKEN`, `SECRET`, `PASS`, `AUTH`) are automatically replaced with `<REDACTED_ON_EXPORT>`.
- **Validation on Import**: Incoming template packs validate schema version 1, confirm valid JSON structures, and verify safe alphanumeric identifiers before writing to SQLite.
