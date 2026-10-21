# Security Policy

## Supported Versions

Security updates are applied to the active development branch and release versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take the security of Kyro seriously. Because Kyro operates with local-system execution capabilities, browser automation, and computer-use synthesis, security defense-in-depth is built into every layer.

If you believe you have found a security vulnerability in Kyro:

1. **Do not disclose the issue publicly** or in public issue trackers.
2. Email your findings directly to **Eshan Roy** at [m.eshanized@gmail.com](mailto:m.eshanized@gmail.com).
3. If possible, encrypt sensitive details or request a PGP key exchange.
4. Alternatively, use GitHub's [Private Vulnerability Reporting](https://github.com/eshanized/Kyro/security/advisories/new) once the repository is published.

### What to Include in Your Report

- Description of the vulnerability and its potential impact.
- Step-by-step reproduction instructions or proof-of-concept (PoC).
- System environment (Operating System, architecture, Kyro version).
- Proposed fix or mitigation (if available).

### Response Timeline

- **Initial Acknowledgment**: Within 48 hours.
- **Triage & Status Assessment**: Within 5 business days.
- **Remediation & Patch**: Timeline agreed upon based on severity and complexity.
- **Public Disclosure**: Coordinated after the fix is released.

## Security Architecture & Defense-in-Depth

Kyro enforces a multi-layered security model:

1. **Local-First & Zero Telemetry**:
   - All conversation state, memories, task timelines, and execution logs reside strictly in local SQLite databases (`WAL` mode).
   - No analytics, telemetry, or remote telemetry pings.

2. **OS Keyring Vault**:
   - Secrets, API keys, and OAuth bearer tokens are segregated from SQLite and persisted directly in OS-level credential stores (macOS Keychain, Linux Secret Service / Keyring, Windows Credential Manager) with authenticated AES-256-GCM fallback encryption.
   - Database exports, routine exports, and IPC serialization strictly redact tokens.

3. **Argument-Aware Executable Policies**:
   - Freeform shell execution commands are parsed into exact `argv` structures using shell lexical analysis.
   - Model-controlled commands cannot bypass whitelist checks using subshell expansions or metacharacter chains.

4. **Human-in-the-Loop Takeover**:
   - Consequential browser clicks, file system modifications outside the workspace root, and sensitive checkpoints (2FA, CAPTCHA, password prompts) trigger mandatory user review or interactive human takeover.

5. **Network & Origin Isolation**:
   - Browser automation and web scraping enforce origin domain allowlists and reject dangerous schemes (`javascript:`, `data:`, `file:`) and cloud metadata endpoints (`169.254.169.254`).

For complete security architecture details, see [docs/architecture/security-boundaries.md](docs/architecture/security-boundaries.md).
