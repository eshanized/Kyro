# Contributing to Kyro

Thank you for your interest in contributing to Kyro! We welcome contributions from developers, designers, technical writers, and testers of all skill levels.

Please take a moment to read this guide to ensure a smooth collaboration process.

---

## Code of Conduct

By participating in this project, you agree to abide by the [Code of Conduct](CODE_OF_CONDUCT.md). Please report any unacceptable behavior to [m.eshanized@gmail.com](mailto:m.eshanized@gmail.com).

---

## Architecture Overview

Kyro is composed of two primary tiers:
- **Desktop Frontend (`kyroui/`)**: React 19, TypeScript, Vite, Tailwind CSS, Lucide icons, Vitest.
- **Native Backend (`libkyro/`)**: Rust, Tauri v2 IPC, SQLite (WAL mode), portable-pty, xcap, enigo, MCP client.

Before making significant architectural changes, review the detailed technical documents in `docs/`:
- [docs/architecture/runtime-overview.md](docs/architecture/runtime-overview.md)
- [docs/architecture/security-boundaries.md](docs/architecture/security-boundaries.md)
- [docs/architecture/connector-system.md](docs/architecture/connector-system.md)

---

## Development Setup

### 1. Prerequisites

- **Node.js**: `v20.x` or higher
- **Rust toolchain**: `1.75.0` or higher (`cargo`, `rustc`, `rustfmt`, `clippy`)
- **System packages** (for Linux):
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

### 2. Getting the Repository

```bash
git clone https://github.com/eshanized/Kyro.git
cd Kyro
npm install
```

### 3. Running Locally

Kyro includes a versatile Makefile for common tasks:

- **Run complete desktop app in dev mode**:
  ```bash
  make dev
  # or: npm run tauri dev
  ```
- **Run Vite frontend only in browser** (for rapid UI styling and prototyping):
  ```bash
  make dev-ui
  # or: npm run dev
  # Available at http://localhost:1420
  ```
- **Run in debug mode with verbose logs**:
  ```bash
  make dev-debug
  ```
- **Run under headless virtual display (Linux / CI)**:
  ```bash
  make dev-headless
  ```

---

## Commit Guidelines

We enforce the [Conventional Commits](https://www.conventionalcommits.org/) specification for clear, automated changelogs.

Format: `<type>(<scope>): <short description>`

Examples:
- `feat(ui): add visual artifact comparison slider`
- `fix(governance): prevent unverified shell expansions in executor`
- `test(audit): add regression tests for element inspection`
- `docs(runtime): update connector authentication flow`
- `refactor(storage): streamline migration query indices`

Common types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`.

---

## Testing & Quality Assurance

Before submitting any Pull Request, all automated checks must pass locally:

### Run the full verification suite
```bash
make verify
```
This automatically executes:
1. `make fmt-check` — Rust code formatting check
2. `make clippy` — Rust Clippy linter with strict warning enforcement (`-D warnings`)
3. `make test` — All Rust unit, integration, and security regression tests
4. `make test-ui` — Frontend Vitest suite and TypeScript compiler typecheck (`tsc --noEmit`)
5. `make build-ui` — Frontend production bundle build

### Individual check commands:
- Frontend tests: `npm test`
- Frontend typecheck: `npx tsc --noEmit`
- Backend check: `cargo check --manifest-path libkyro/Cargo.toml --all-targets`
- Backend tests: `cargo test --manifest-path libkyro/Cargo.toml --all-features`

---

## Pull Request Checklist

When submitting a PR, please make sure:

1. [ ] Branch is based off the latest `master` branch.
2. [ ] All tests pass (`make verify`).
3. [ ] Code follows existing style and naming conventions.
4. [ ] Any new feature is backed by automated tests (frontend Vitest and/or Rust integration tests).
5. [ ] Relevant documentation in `docs/` or `README.md` is updated.
6. [ ] PR description clearly explains the motivation, changes made, and test evidence.

Thank you for helping make Kyro better!
