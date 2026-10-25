---
schema_version: 1
id: software-change
name: Software Change Pipeline
description: Investigate, implement, verify, and review a code change in an isolated worktree
avatar: wrench
enabled: true
concurrency: 2
on_failure: abort
tasks:
  - task_id: investigate
    agent_id: forge-devops
    objective: "Investigate the codebase and map the affected modules, tests, and manifests for: {{objective}}"
    depends_on: []
    optional: false
    isolated_worktree: false
    retries: 0
  - task_id: implement
    agent_id: forge-devops
    objective: "Implement a minimal, production-quality change for: {{objective}}. Record every file you modify."
    depends_on:
      - investigate
    optional: false
    isolated_worktree: true
    retries: 1
  - task_id: verify
    agent_id: test-regression-specialist
    objective: "Run the relevant build, tests, and diagnostics in the isolated worktree and verify the change for: {{objective}}. Report verification results truthfully."
    depends_on:
      - implement
    optional: false
    isolated_worktree: true
    retries: 1
  - task_id: review
    agent_id: code-reviewer
    objective: "Review the produced diff and verify the change is safe to integrate for: {{objective}}. List risks and required follow-ups."
    depends_on:
      - verify
    optional: false
    isolated_worktree: true
    retries: 0
  - task_id: documentation
    agent_id: documentation-reviewer
    objective: "Check and update relevant documentation, API contracts, and user guides for: {{objective}}."
    depends_on:
      - review
    optional: true
    isolated_worktree: true
    retries: 0
---

# Software Change Pipeline

The default developer workflow coordinating specialized Kyro agents:
1. Investigation feeds implementation: Forge investigates the project and maps affected modules and tests.
2. Isolated implementation: Forge implements the targeted change inside an isolated worktree without touching the user's working copy.
3. Verification: The Test Specialist examines the candidate changes, runs focused tests, and records verification evidence.
4. Code Review: The Code Reviewer inspects the candidate diff and verification evidence, assessing safety and edge cases.
5. Documentation: The Documentation Specialist verifies and aligns documentation, API contracts, and guides.

All candidate work remains isolated in a dedicated Git worktree (or shadow copy) until human-reviewed reconciliation.
