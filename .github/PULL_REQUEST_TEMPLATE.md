## Description

Please include a summary of the change and which issue it fixes or feature it implements. Include relevant motivation and context.

Fixes #(issue)

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Performance optimization or refactor

## Subsystems Affected

- [ ] `kyroui` (Frontend React/TypeScript)
- [ ] `libkyro::agent` (Loop engine, vision, router)
- [ ] `libkyro::governance` (Authorization, policies, takeover)
- [ ] `libkyro::computer_use` (Capture, input, overlay, grounding)
- [ ] `libkyro::connector` / `libkyro::mcp` (Connectors, MCP clients)
- [ ] `libkyro::audit` / `libkyro::project` (UI Audit, Dev Workbench, Worktrees)
- [ ] `libkyro::storage` (SQLite migrations, Vault)

## Verification Checklist

- [ ] I have executed `make verify` and all checks passed:
  - [ ] Rust formatting check (`cargo fmt`)
  - [ ] Rust clippy (`cargo clippy --all-targets --all-features -- -D warnings`)
  - [ ] Rust unit & integration tests (`cargo test`)
  - [ ] Frontend tests (`npm test`)
  - [ ] Frontend typecheck (`tsc --noEmit`)
  - [ ] Frontend production build (`npm run build`)
- [ ] I have added automated tests for my changes
- [ ] I have updated relevant documentation in `docs/` or `README.md`
- [ ] My commits adhere to [Conventional Commits](https://www.conventionalcommits.org/) standards
