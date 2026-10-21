import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { DevWorkbenchModal } from "../components/DevWorkbenchModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("Kyro Dev Workbench Modal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("handles project discovery and dev server controls", async () => {
    const mockDiscovery = {
      searched_root: "/home/snigdha/Desktop/Kyro",
      candidates: [
        {
          name: "kyroui",
          path: "/home/snigdha/Desktop/Kyro/kyroui",
          kind: "Node",
          package_manager: "npm",
          is_git_repo: true,
          scripts: ["dev", "build", "test"],
          name_matched: true,
        },
      ],
      exact_match: null,
      error: null,
    };

    const mockDevServers = [
      {
        id: "server-1",
        pid: 12345,
        working_directory: "/home/snigdha/Desktop/Kyro/kyroui",
        command: "npm run dev",
        status: "ready" as const,
        ready_url: "http://localhost:5173",
        port: 5173,
        framework: "Vite",
        started_at: 1700000000,
      },
    ];

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_dev_servers") {
        return Promise.resolve(mockDevServers);
      }
      if (cmd === "discover_projects") {
        return Promise.resolve(mockDiscovery);
      }
      if (cmd === "start_dev_server") {
        return Promise.resolve(mockDevServers[0]);
      }
      return Promise.resolve(null);
    });

    const handleClose = vi.fn();
    render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="projects" />);

    expect(screen.getByText("Kyro Developer Workbench")).toBeInTheDocument();
    expect(screen.getByText("Projects & Dev Servers")).toBeInTheDocument();

    // Trigger Discover Projects
    const discoverBtn = screen.getByText("Discover Projects");
    fireEvent.click(discoverBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("discover_projects", expect.any(Object));
      expect(screen.getByText("kyroui")).toBeInTheDocument();
      expect(screen.getByText("Server #server-1")).toBeInTheDocument();
      expect(screen.getByText("Port :5173")).toBeInTheDocument();
    });
  });

  it("handles checkpoint creation, unified diff rendering and rollback", async () => {
    const mockCheckpoint = {
      checkpoint_id: "cp-789",
      workspace_path: "/home/snigdha/Desktop/Kyro",
      description: "Refactor router modules",
      created_at: 1700000000,
      pre_existing_dirty_files: [],
      modified_files: [
        {
          file_path: "src/router.rs",
          new_content: "+ pub fn list_models() {}",
          additions: 12,
          deletions: 2,
          unified_diff: "@@ -10,3 +10,13 @@\n+ pub fn list_models() {}\n- old_function();",
          risk_assessment: "low" as const,
        },
      ],
      status: "active" as const,
    };

    const mockRollback = {
      checkpoint_id: "cp-789",
      files_reverted: ["src/router.rs"],
      conflicts_detected: [],
      status: "success" as const,
    };

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "create_code_checkpoint") {
        return Promise.resolve(mockCheckpoint);
      }
      if (cmd === "rollback_code_checkpoint") {
        return Promise.resolve(mockRollback);
      }
      return Promise.resolve([]);
    });

    const handleClose = vi.fn();
    render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="checkpoints" />);

    expect(screen.getByText("Checkpoints & Diffs")).toBeInTheDocument();

    // Create Checkpoint
    const saveBtn = screen.getByText("Save Checkpoint");
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_code_checkpoint", expect.any(Object));
      expect(screen.getByText("Refactor router modules")).toBeInTheDocument();
      expect(screen.getByText("src/router.rs")).toBeInTheDocument();
      expect(screen.getByText("+12")).toBeInTheDocument();
    });

    // 1-Click Rollback
    const rollbackBtn = screen.getByText("1-Click Rollback");
    fireEvent.click(rollbackBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("rollback_code_checkpoint", { checkpoint: mockCheckpoint });
      expect(screen.getByText(/Checkpoint Rollback SUCCESS/i)).toBeInTheDocument();
    });
  });

  it("diagnoses compiler error and provides actionable fix in Build Doctor", async () => {
    const mockDiagnostic = {
      root_cause_error: "cannot find value `non_existent_var` in this scope",
      file: "src/main.rs",
      line_number: 42,
      error_code: "E0425",
      category: "syntax" as const,
      suggested_repair: "let non_existent_var = 42;",
      reproduced: true,
    };

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "diagnose_build_failure") {
        return Promise.resolve(mockDiagnostic);
      }
      return Promise.resolve([]);
    });

    const handleClose = vi.fn();
    render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="doctor" />);

    expect(screen.getByText("Build Doctor")).toBeInTheDocument();

    const runDoctorBtn = screen.getByText("Run Build Doctor");
    fireEvent.click(runDoctorBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("diagnose_build_failure", expect.any(Object));
      expect(screen.getByText("E0425")).toBeInTheDocument();
      expect(screen.getByText("src/main.rs:42")).toBeInTheDocument();
      expect(screen.getByText("let non_existent_var = 42;")).toBeInTheDocument();
    });
  });

  it("handles responsive UI audit baselines and visual regression comparison", async () => {
    const mockBaseline = {
      project_path: "/home/snigdha/Desktop/Kyro",
      route: "/",
      viewport: "desktop",
      base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      created_at: 1700000000,
    };

    const mockComparison = {
      has_regression: false,
      diff_percent: 0.0,
      route: "/",
      viewport: "desktop",
      description: "Visual appearance matches trusted baseline within acceptable tolerance.",
    };

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "get_audit_baseline") {
        return Promise.resolve(mockBaseline);
      }
      if (cmd === "compare_visual_baseline") {
        return Promise.resolve(mockComparison);
      }
      return Promise.resolve([]);
    });

    const handleClose = vi.fn();
    render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="audit" />);

    expect(screen.getByText("UI Audit & Baselines")).toBeInTheDocument();

    // Inspect stored baseline
    const inspectBtn = screen.getByText("Inspect Stored Baseline");
    fireEvent.click(inspectBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_audit_baseline", expect.any(Object));
      expect(screen.getByText(/Baseline for Route \/ \(desktop\)/)).toBeInTheDocument();
    });

    // Compare against baseline
    const compareBtn = screen.getByText("Compare Against Baseline");
    fireEvent.click(compareBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("compare_visual_baseline", expect.any(Object));
      expect(screen.getByText("Visual Match Passed")).toBeInTheDocument();
      expect(screen.getByText("Diff: 0.00%")).toBeInTheDocument();
    });
  });

  it("initializes multi-agent swarm workspace and reconciles isolated worktree changes", async () => {
    const mockWorkspace = {
      workspace_id: "ws-999",
      parent_run_id: "run-100",
      project_path: "/home/snigdha/Desktop/Kyro",
      tasks: [
        {
          task_id: "task-1",
          role: "repository_investigator" as const,
          objective: "Map codebase architecture",
          dependencies: [],
          status: "completed" as const,
          isolated_worktree_path: "/home/snigdha/Desktop/Kyro/.kyro-worktrees/repository_investigator",
        },
        {
          task_id: "task-2",
          role: "implementation_engineer" as const,
          objective: "Implement visual regression engine",
          dependencies: ["task-1"],
          status: "completed" as const,
          isolated_worktree_path: "/home/snigdha/Desktop/Kyro/.kyro-worktrees/implementation_engineer",
        },
      ],
      status: "active" as const,
    };

    const mockReconciliation = {
      workspace_id: "ws-999",
      changes_detected: true,
      modified_files: ["src/audit/regression.rs"],
      unified_diff: "+++ b/src/audit/regression.rs\n@@ -0,0 +1,50 @@\n+pub struct VisualRegressionEngine;",
      conflicts_detected: [],
      can_safely_merge: true,
      reconciliation_summary: "Clean reconciliation with 0 merge conflicts across isolated worktrees.",
    };

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "create_multi_agent_workspace") {
        return Promise.resolve(mockWorkspace);
      }
      if (cmd === "reconcile_multi_agent_workspace") {
        return Promise.resolve(mockReconciliation);
      }
      return Promise.resolve([]);
    });

    const handleClose = vi.fn();
    render(<DevWorkbenchModal isOpen={true} onClose={handleClose} initialTab="workspace" />);

    expect(screen.getByText("Multi-Agent Workspace")).toBeInTheDocument();

    // Create workspace
    const createBtn = screen.getByText("Initialize Specialist Workspace");
    fireEvent.click(createBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_multi_agent_workspace", expect.any(Object));
      expect(screen.getByText(/repository investigator/i)).toBeInTheDocument();
      expect(screen.getByText(/implementation engineer/i)).toBeInTheDocument();
    });

    // Reconcile
    const reconcileBtn = screen.getByText("Reconcile Worktree Changes");
    fireEvent.click(reconcileBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reconcile_multi_agent_workspace", expect.any(Object));
      expect(screen.getByText("Safe To Merge")).toBeInTheDocument();
      expect(screen.getByText("1 file(s) modified")).toBeInTheDocument();
      expect(screen.getByText("Clean reconciliation with 0 merge conflicts across isolated worktrees.")).toBeInTheDocument();
    });
  });
});
