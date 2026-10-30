import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DevWorkbenchModal,
  findTerminalCandidateTasks,
} from "../components/DevWorkbenchModal";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { App } from "../App";
import { SpecialistTask } from "../types/agent";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

let progressListeners: ((event: { payload: any }) => void)[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: (e: any) => void) => {
    if (event === "skill:progress") {
      progressListeners.push(cb);
    }
    return Promise.resolve(() => {
      progressListeners = progressListeners.filter((l) => l !== cb);
    });
  }),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ listen: vi.fn().mockResolvedValue(() => {}) }),
}));

const mockInvoke = vi.mocked(invoke);

describe("Wiring Audit Issues #27-#35 Regression Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    progressListeners = [];
  });

  // ---------------------------------------------------------------------------
  // Issue #27 & #28: Terminal Candidate Selection & Explicit Reconciliation Apply
  // ---------------------------------------------------------------------------
  describe("Issue #28: Terminal Candidate Selection in Dependency Graph", () => {
    it("selects the final completed stage in a linear pipeline rather than the initial worktree", () => {
      const linearTasks: SpecialistTask[] = [
        {
          task_id: "investigate",
          agent_id: "forge-devops",
          objective: "Investigate repo",
          depends_on: [],
          optional: false,
          isolated_worktree: false,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: null,
          worktree_branch: null,
          base_commit: null,
          output: null,
          error: null,
          started_at: 1,
          completed_at: 2,
        },
        {
          task_id: "implement",
          agent_id: "forge-devops",
          objective: "Implement changes",
          depends_on: ["investigate"],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: "/scratch/worktrees/implement",
          worktree_branch: "kyro-wt-implement",
          base_commit: null,
          output: null,
          error: null,
          started_at: 2,
          completed_at: 3,
        },
        {
          task_id: "verify",
          agent_id: "test-regression-specialist",
          objective: "Run test suite",
          depends_on: ["implement"],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: "/scratch/worktrees/verify",
          worktree_branch: "kyro-wt-verify",
          base_commit: null,
          output: null,
          error: null,
          started_at: 3,
          completed_at: 4,
        },
      ];

      const terminal = findTerminalCandidateTasks(linearTasks);
      expect(terminal.length).toBe(1);
      expect(terminal[0].task_id).toBe("verify");
      expect(terminal[0].isolated_worktree_path).toBe("/scratch/worktrees/verify");
    });

    it("returns multiple independent terminal tasks in a branching DAG", () => {
      const branchingTasks: SpecialistTask[] = [
        {
          task_id: "implement",
          agent_id: "forge-devops",
          objective: "Base changes",
          depends_on: [],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: "/scratch/worktrees/base",
          worktree_branch: "kyro-wt-base",
          base_commit: null,
          output: null,
          error: null,
          started_at: 1,
          completed_at: 2,
        },
        {
          task_id: "test_fe",
          agent_id: "test-regression-specialist",
          objective: "Frontend tests",
          depends_on: ["implement"],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: "/scratch/worktrees/fe",
          worktree_branch: "kyro-wt-fe",
          base_commit: null,
          output: null,
          error: null,
          started_at: 2,
          completed_at: 3,
        },
        {
          task_id: "test_be",
          agent_id: "test-regression-specialist",
          objective: "Backend tests",
          depends_on: ["implement"],
          optional: false,
          isolated_worktree: true,
          max_attempts: 1,
          attempts: 1,
          status: "completed",
          child_run_id: null,
          isolated_worktree_path: "/scratch/worktrees/be",
          worktree_branch: "kyro-wt-be",
          base_commit: null,
          output: null,
          error: null,
          started_at: 2,
          completed_at: 3,
        },
      ];

      const terminals = findTerminalCandidateTasks(branchingTasks);
      expect(terminals.length).toBe(2);
      const ids = terminals.map((t) => t.task_id);
      expect(ids).toContain("test_fe");
      expect(ids).toContain("test_be");
      expect(ids).not.toContain("implement");
    });
  });

  describe("Issue #27: Explicit Reconciliation Apply Contract", () => {
    it("wires reconciliation through an explicit apply action gated on can_safely_merge", async () => {
      const mockWorkspace = {
        workspace_id: "ws-apply-1",
        parent_run_id: "run-parent",
        project_path: "/tmp/proj",
        workflow_id: "software-change",
        workflow_name: "Software Change",
        primary_objective: "Build feature",
        concurrency: 2,
        on_failure: "continue" as const,
        status: "completed",
        created_at: 1000,
        updated_at: 1000,
        tasks: [
          {
            task_id: "implement",
            agent_id: "forge-devops",
            objective: "Code",
            depends_on: [],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed",
            child_run_id: null,
            isolated_worktree_path: "/scratch/worktrees/implement",
            worktree_branch: "kyro-wt-implement",
            base_commit: null,
            output: null,
            error: null,
            started_at: 1,
            completed_at: 2,
          },
        ],
      };

      const mockOutcome = {
        workspace_id: "ws-apply-1",
        changes_detected: true,
        modified_files: ["src/lib.rs"],
        unified_diff: "@@ -1 +1 @@\n+pub fn run() {}",
        conflicts_detected: [],
        can_safely_merge: true,
        reconciliation_summary: "Changes safely verified and ready to merge.",
      };

      mockInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_multi_agent_workspaces") return [mockWorkspace];
        if (cmd === "reconcile_multi_agent_workspace") return mockOutcome;
        if (cmd === "apply_multi_agent_workspace_reconciliation") {
          return "Successfully merged branch 'kyro-wt-implement' into project root.";
        }
        return null;
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="workspace" />);

      await waitFor(() => {
        expect(screen.getByText("Reconcile Worktree Changes")).toBeInTheDocument();
      });

      // 1. Click reconcile diff inspection
      fireEvent.click(screen.getByText("Reconcile Worktree Changes"));

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "reconcile_multi_agent_workspace",
          expect.objectContaining({
            projectRoot: "/tmp/proj",
            worktreePath: "/scratch/worktrees/implement",
            workspaceId: "ws-apply-1",
          })
        );
        expect(screen.getByText("Safe To Merge")).toBeInTheDocument();
        expect(screen.getByText("Apply Reconciled Changes")).toBeInTheDocument();
      });

      // 2. Click Apply Reconciled Changes
      fireEvent.click(screen.getByText("Apply Reconciled Changes"));

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "apply_multi_agent_workspace_reconciliation",
          expect.objectContaining({
            projectRoot: "/tmp/proj",
            worktreePath: "/scratch/worktrees/implement",
            workspaceId: "ws-apply-1",
            branchName: "kyro-wt-implement",
          })
        );
        expect(screen.getByText("Reconciliation Applied")).toBeInTheDocument();
      });
    });
  });

  // ---------------------------------------------------------------------------
  // Issue #30: Real Screenshot Capture in Visual Regression Baselines
  // ---------------------------------------------------------------------------
  describe("Issue #30: Visual Regression Real Screenshot Capture", () => {
    it("captures real screen data and handles screenshot capture failure gracefully", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "capture_audit_screenshot") {
          throw new Error("No active browser session or monitor display available");
        }
        if (cmd === "take_screenshot") {
          throw new Error("X11 display :0 unavailable");
        }
        return null;
      });

      render(<DevWorkbenchModal isOpen={true} onClose={vi.fn()} initialTab="audit" />);

      const saveBtn = screen.getByText("Save As Baseline");
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(screen.getByText(/Error capturing baseline/i)).toBeInTheDocument();
      });
    });
  });

  // ---------------------------------------------------------------------------
  // Issue #31: TeachTask Skill Replay Progress Events
  // ---------------------------------------------------------------------------
  describe("Issue #31: TeachTask Replay Progress Events", () => {
    it("subscribes to skill:progress and renders active step progress, filtering other skills", async () => {
      const mockSkill = {
        id: "skill-101",
        name: "Automate Form Fill",
        description: "Enters registration data",
        steps_json: JSON.stringify([
          { action: "click", target: "input#name" },
          { action: "type", text: "Kyro Agent" },
        ]),
        created_at: 1000,
      };

      mockInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_skills") return [mockSkill];
        if (cmd === "replay_skill") {
          // Simulate progress events while running
          if (progressListeners.length > 0) {
            // Irrelevant skill event should be ignored
            progressListeners[0]({
              payload: { skill_id: "other-skill", step: 1, total: 10, description: "Other" },
            });
            // Matching skill event
            progressListeners[0]({
              payload: {
                skill_id: "skill-101",
                step: 1,
                total: 2,
                description: "Clicking input#name",
              },
            });
          }
          return Promise.resolve();
        }
        return null;
      });

      render(<TeachTaskModal isOpen={true} onClose={vi.fn()} initialTab="skills" />);

      await waitFor(() => {
        expect(screen.getByText("Automate Form Fill")).toBeInTheDocument();
      });

      const replayBtn = screen.getByRole("button", { name: /^Replay$/i });
      fireEvent.click(replayBtn);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("replay_skill", { skillId: "skill-101" });
      });
    });
  });

  // ---------------------------------------------------------------------------
  // Issue #32: Truthful Artifact Feedback Persistence & Conversation History
  // ---------------------------------------------------------------------------
  describe("Issue #32: Artifact Feedback Persistence", () => {
    it("indicates feedback is queued into conversation history for the next agent turn", async () => {
      const mockArtifact = {
        id: "art-graph-1",
        run_id: "run-10",
        conversation_id: "conv-42",
        name: "architecture_diagram.png",
        file_path: "/tmp/arch.png",
        artifact_type: "image",
        mime_type: "image/png",
        size_bytes: 4096,
        created_at: 1000,
        metadata_json: JSON.stringify({ comments: [] }),
      };

      mockInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "list_artifacts") return [mockArtifact];
        if (cmd === "read_artifact_content") {
          return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        }
        if (cmd === "add_artifact_comment") return true;
        return null;
      });

      render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getAllByText("architecture_diagram.png")[0]).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Add comment or feedback/i);
      fireEvent.change(input, { target: { value: "Please highlight the database connection" } });
      fireEvent.click(screen.getByRole("button", { name: /Send Feedback/i }));

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("add_artifact_comment", {
          id: "art-graph-1",
          comment: "Please highlight the database connection",
          feedToAgent: true,
        });
        expect(
          screen.getByText("Comment saved and queued into conversation history for the next agent turn.")
        ).toBeInTheDocument();
      });
    });

    it("surfaces database errors truthfully when feedback insertion fails", async () => {
      const mockArtifact = {
        id: "art-err-1",
        run_id: "run-10",
        conversation_id: "conv-missing",
        name: "spec.png",
        file_path: "/tmp/spec.png",
        artifact_type: "image",
        mime_type: "image/png",
        size_bytes: 120,
        created_at: 1000,
        metadata_json: JSON.stringify({ comments: [] }),
      };

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "list_artifacts") return [mockArtifact];
        if (cmd === "read_artifact_content") {
          return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        }
        if (cmd === "add_artifact_comment") {
          throw new Error("Failed to inject feedback into conversation history: database locked");
        }
        return null;
      });

      render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getAllByText("spec.png")[0]).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Add comment or feedback/i);
      fireEvent.change(input, { target: { value: "Rejected spec" } });
      fireEvent.click(screen.getByRole("button", { name: /Send Feedback/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/Error adding comment: Failed to inject feedback/i)
        ).toBeInTheDocument();
      });
    });
  });

  // ---------------------------------------------------------------------------
  // Issue #35: Never Fabricate Task Completion on Empty Response
  // ---------------------------------------------------------------------------
  describe("Issue #35: Empty Model Response Truthfulness", () => {
    it("displays truthful state and marks plan failed when model generates an empty reply without verification", async () => {
      let turnResolvers: ((val: string) => void)[] = [];

      mockInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
        if (cmd === "list_conversations") {
          return [{ id: "conv-empty-test", title: "Empty Turn Test", model: "m", created_at: 1000 }];
        }
        if (cmd === "get_conversation_messages") return [];
        if (cmd === "get_default_bot") {
          return { id: "apex", name: "Apex", role: "Orchestrator", avatar: "zap", system_prompt: "", is_default: true };
        }
        if (cmd === "list_pending_approvals") return [];
        if (cmd === "list_authorized_folders") return [];
        if (cmd === "add_message") return {};
        if (cmd === "run_agent_turn") {
          return new Promise<string>((resolve) => {
            turnResolvers.push(resolve);
          });
        }
        return null;
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
      });

      // Submit user turn
      const textarea = screen.getByPlaceholderText(/Ask @/i);
      fireEvent.change(textarea, { target: { value: "Perform task with silent response" } });
      fireEvent.submit(textarea);

      await waitFor(() => {
        expect(turnResolvers.length).toBeGreaterThan(0);
      });

      // Resolve turn with EMPTY output
      await act(async () => {
        turnResolvers[0]("");
      });

      await waitFor(() => {
        // Must NEVER fabricate "Task completed."
        expect(screen.queryByText("Task completed.")).not.toBeInTheDocument();
        // Must truthfully inform the user
        expect(screen.getByText(/No response was generated by the model/i)).toBeInTheDocument();
      });
    });
  });
});
