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
    agent_id: forge-devops
    objective: "Run the relevant build, tests, and diagnostics in the isolated worktree and verify the change for: {{objective}}. Report verification results truthfully."
    depends_on:
      - implement
    optional: false
    isolated_worktree: true
    retries: 1
  - task_id: review
    agent_id: apex-orchestrator
    objective: "Review the produced diff and verify the change is safe to integrate for: {{objective}}. List risks and required follow-ups."
    depends_on:
      - verify
    optional: false
    isolated_worktree: true
    retries: 0
---

# Software Change Pipeline

The default developer workflow. Each stage is a real delegated agent run:
investigation feeds implementation, implementation is verified by a separate
test stage, and integration requires review. Implementation, verification, and
review execute in an isolated Git worktree (or shadow copy) and produce diffs
for reviewed reconciliation - nothing is merged into the user's tree
automatically.

Adapt or replace this workflow by editing the manifest; stages are optional and
can be assigned to any enabled agent, including custom agents.
