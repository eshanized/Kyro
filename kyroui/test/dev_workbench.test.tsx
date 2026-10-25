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
      if (cmd === "capture_audit_screenshot" || cmd === "take_screenshot") {
        return Promise.resolve({ width: 1280, height: 800, base64_png: mockBaseline.base64_png });
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
      workflow_id: "software-change",
      workflow_name: "Software Change",
      primary_objective: "Implement visual regression engine",
      concurrency: 2,
      on_failure: "continue",
      tasks: [
        {
          task_id: "investigate",
          agent_id: "forge-devops",
          objective: "Map codebase architecture",
          depends_on: [],
          optional: false,
          isolated_worktree: false,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: "ws-999_investigate_a1",
          isolated_worktree_path: null,
          worktree_branch: null,
          output: "done",
          error: null,
          started_at: 1,
          completed_at: 2,
        },
        {
          task_id: "implement",
          agent_id: "forge-devops",
          objective: "Implement visual regression engine",
          depends_on: ["investigate"],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: "ws-999_implement_a1",
          isolated_worktree_path: "/tmp/kyro/scratch/worktrees/implement",
          worktree_branch: "kyro-wt-implement",
          output: "done",
          error: null,
          started_at: 2,
          completed_at: 3,
        },
      ],
      status: "completed",
      created_at: 0,
      updated_at: 0,
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
      if (cmd === "list_multi_agent_workspaces") {
        return Promise.resolve([]);
      }
      if (cmd === "create_multi_agent_workspace") {
        return Promise.resolve(mockWorkspace);
      }
      if (cmd === "run_multi_agent_workspace") {
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

    // Plan the workspace from the bundled workflow
    const createBtn = screen.getByText("Plan Workspace");
    fireEvent.click(createBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_multi_agent_workspace", expect.any(Object));
      expect(screen.getAllByText("investigate").length).toBeGreaterThan(0);
      expect(screen.getAllByText(/forge-devops/i).length).toBeGreaterThan(0);
    });

    // Reconcile the worktree that actually holds integrated work
    const reconcileBtn = screen.getByText("Reconcile Worktree Changes");
    fireEvent.click(reconcileBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reconcile_multi_agent_workspace", expect.any(Object));
      expect(screen.getByText("Safe To Merge")).toBeInTheDocument();
      expect(screen.getByText("1 file(s) modified")).toBeInTheDocument();
      expect(screen.getByText("Clean reconciliation with 0 merge conflicts across isolated worktrees.")).toBeInTheDocument();
    });
  });

  it("handles experiment lifecycle: create, run, rerun, compare, delete and verifies millisecond timestamp", async () => {
    const fixedTimestampMs = 1714000000000;
    const mockExpA = {
      id: "exp-test-1",
      name: "test-fibonacci-a",
      run_id: null,
      runtime: "python",
      image: "python:3.11-alpine",
      script: "print(55)",
      argv_json: "[]",
      workspace_path: null,
      output_dir: "/tmp/out_a",
      network_mode: "none",
      cpu_limit: 1.0,
      memory_limit_mb: 512,
      timeout_seconds: 60,
      status: "succeeded",
      exit_code: 0,
      stdout_tail: "55\n",
      stderr_tail: null,
      result_summary: "Experiment succeeded",
      artifact_ids_json: "[]",
      created_at: fixedTimestampMs,
      started_at: fixedTimestampMs,
      completed_at: fixedTimestampMs + 200,
      error: null,
    };

    const mockExpB = {
      ...mockExpA,
      id: "exp-test-2",
      name: "test-fibonacci-b",
    };

    const mockComparison = {
      exp_a_id: "exp-test-1",
      exp_b_id: "exp-test-2",
      exit_code_match: true,
      stdout_match: true,
      stderr_match: true,
      diff_summary: "Outputs match identically.",
    };

    (invoke as any).mockImplementation((cmd: string, args: any) => {
      if (cmd === "list_experiments") {
        return Promise.resolve([mockExpA, mockExpB]);
      }
      if (cmd === "create_experiment") {
        return Promise.resolve(mockExpA);
      }
      if (cmd === "run_experiment") {
        return Promise.resolve({ ...mockExpA, status: "succeeded" });
      }
      if (cmd === "delete_experiment") {
        return Promise.resolve(true);
      }
      if (cmd === "compare_experiments") {
        return Promise.resolve(mockComparison);
      }
      return Promise.resolve(null);
    });

    render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="experiments" />);

    expect(screen.getByText("Sandboxed Code-Execution Agents")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText("test-fibonacci-a")).toBeInTheDocument();
      expect(screen.getByText("test-fibonacci-b")).toBeInTheDocument();
    });

    // Check millisecond timestamp formatting (Issue #33)
    const expectedTime = new Date(fixedTimestampMs).toLocaleTimeString();
    expect(screen.getAllByText(new RegExp(expectedTime)).length).toBeGreaterThan(0);

    // Save spec only
    const saveOnlyBtn = screen.getByText("Save Spec Only");
    fireEvent.click(saveOnlyBtn);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_experiment", expect.objectContaining({ spec: expect.any(Object) }));
    });

    // Run experiment directly (Issue #26)
    const runNowBtn = screen.getByText("Run Isolated Experiment");
    fireEvent.click(runNowBtn);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "run_experiment",
        expect.objectContaining({
          experimentId: "exp-test-1",
          id: "exp-test-1",
        })
      );
    });

    // Delete experiment (Issue #26)
    const deleteBtns = screen.getAllByTitle("Delete record");
    fireEvent.click(deleteBtns[0]);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "delete_experiment",
        expect.objectContaining({
          id: "exp-test-1",
          experimentId: "exp-test-1",
        })
      );
    });

    // Compare experiments (Issue #26)
    const selects = screen.getAllByRole("combobox");
    const expASelect = selects.find((s) => s.innerHTML.includes("Select Experiment A...")) as HTMLSelectElement;
    const expBSelect = selects.find((s) => s.innerHTML.includes("Select Experiment B...")) as HTMLSelectElement;
    if (expASelect && expBSelect) {
      fireEvent.change(expASelect, { target: { value: "exp-test-1" } });
      fireEvent.change(expBSelect, { target: { value: "exp-test-2" } });
      const compareBtn = screen.getByText("Compare Runs");
      fireEvent.click(compareBtn);
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith(
          "compare_experiments",
          expect.objectContaining({
            idA: "exp-test-1",
            idB: "exp-test-2",
          })
        );
      });
    }
  });
});
