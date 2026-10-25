import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App, getModelDisplayInfo } from "../App";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

let eventListeners: Record<string, (event: any) => void> = {};

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((eventName: string, callback: (event: any) => void) => {
    eventListeners[eventName] = callback;
    return Promise.resolve(() => {
      delete eventListeners[eventName];
    });
  }),
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    open = vi.fn();
    loadAddon = vi.fn();
    writeln = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = vi.fn();
  },
}));

describe("Phase 3 & 7 — Plan Semantics and Model Persistence", () => {
  const mockConversations = [
    {
      id: "conv_test_1",
      title: "Plan Test Task",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    eventListeners = {};
    window.open = vi.fn();

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_conversations") return Promise.resolve(mockConversations);
      if (cmd === "get_messages" || cmd === "get_conversation_messages") return Promise.resolve([]);
      if (cmd === "get_default_bot") {
        return Promise.resolve({
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        });
      }
      if (cmd === "get_selected_model") {
        return Promise.resolve("meta/llama-3.3-70b-instruct");
      }
      if (cmd === "save_selected_model") {
        return Promise.resolve("meta/llama-3.3-70b-instruct");
      }
      if (cmd === "get_takeover_status") {
        return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
      }
      if (cmd === "get_pending_approvals" || cmd === "list_pending_approvals") return Promise.resolve([]);
      if (cmd === "get_api_keys") return Promise.resolve({ nvidia_configured: true });
      if (cmd === "get_governance_policy") return Promise.resolve({});
      if (cmd === "list_connector_catalog") return Promise.resolve([]);
      if (cmd === "list_connected_accounts") return Promise.resolve([]);
      if (cmd === "list_mcp_servers") return Promise.resolve([]);
      if (cmd === "get_mcp_server_statuses") return Promise.resolve({});
      if (cmd === "get_timeline_steps") return Promise.resolve([]);
      if (cmd === "run_agent_turn") return Promise.resolve("Task finished without running verification.");
      return Promise.resolve(null);
    });
  });

  describe("Issue G: Model Selection & Persistence", () => {
    it("loads persisted model preference from SQLite settings on startup", async () => {
      render(<App />);

      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith("get_selected_model");
      });

      // Verify header / sidebar display shows the loaded model
      await waitFor(() => {
        expect(screen.getByText(/Llama 3.3 Instruct/i)).toBeInTheDocument();
        expect(screen.getByText("70B")).toBeInTheDocument();
      });
    });

    it("maps model identifiers to friendly display badges correctly", () => {
      const nemotron = getModelDisplayInfo("nvidia/nemotron-3-ultra-550b-a55b");
      expect(nemotron.label).toBe("Nemotron-3 Ultra");
      expect(nemotron.badge).toBe("550B");
      expect(nemotron.isNvidia).toBe(true);

      const llama = getModelDisplayInfo("meta/llama-3.3-70b-instruct");
      expect(llama.label).toBe("Llama 3.3 Instruct");
      expect(llama.badge).toBe("70B");
      expect(llama.isNvidia).toBe(false);

      const sonnet = getModelDisplayInfo("anthropic/claude-3-5-sonnet");
      expect(sonnet.label).toBe("Claude 3.5 Sonnet");
      expect(sonnet.badge).toBe("Anthropic");
    });
  });

  describe("Issue C: Execution Plan & Verification Semantics", () => {
    it("initializes consequential task plans with empty accomplished claims and pending steps", async () => {
      let turnResolve: (value: string) => void;
      const turnPromise = new Promise<string>((resolve) => {
        turnResolve = resolve;
      });
      (invoke as any).mockImplementation((cmd: string) => {
        if (cmd === "run_agent_turn") return turnPromise;
        if (cmd === "get_selected_model") return Promise.resolve("nvidia/nemotron-3-ultra-550b-a55b");
        if (cmd === "list_conversations") return Promise.resolve(mockConversations);
        if (cmd === "get_conversation_messages") return Promise.resolve([]);
        if (cmd === "get_default_bot") {
          return Promise.resolve({
            id: "apex",
            name: "Apex",
            role: "Strategic Orchestrator",
            avatar: "zap",
            system_prompt: "",
            is_default: true,
          });
        }
        return Promise.resolve(null);
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Ask @/i);
      fireEvent.change(input, { target: { value: "Please clean and organize all download files" } });
      fireEvent.submit(input);

      // Verify Execution Plan card appears
      await waitFor(() => {
        // Bug 5 contract: client template → "Suggested Workflow".
        expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
      });

      // Accomplished MUST start empty (NO fabricated "Verified permissions" or "Task verified")
      expect(
        screen.queryByText("Accomplished Actions:")
      ).not.toBeInTheDocument();

      // Planned steps are rendered without premature completion
      expect(screen.getAllByText(/Inspect and analyze relevant resources and parameters/i).length).toBeGreaterThan(0);
      expect(screen.getAllByText(/Verify execution results and validate state integrity/i).length).toBeGreaterThan(0);

      // Complete turn without verification tool calls
      turnResolve!("All clean.");

      // After turn completion without verification tool:
      await waitFor(() => {
        // Plan status is unverified (NOT blindly completed)
        // Status badge text (no parentheses in the new card).
        expect(screen.getByText("unverified")).toBeInTheDocument();
        expect(screen.getByText(/Explicit verification check not executed/i)).toBeInTheDocument();
      });
    });

    it("marks verification completed only when a verification tool actually executed", async () => {
      let runTurnPromiseResolve: (value: string) => void;
      const runTurnPromise = new Promise<string>((resolve) => {
        runTurnPromiseResolve = resolve;
      });

      let capturedRunId: string | null = null;
      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "run_agent_turn") {
          capturedRunId = args.runId;
          return runTurnPromise;
        }
        if (cmd === "get_selected_model") return Promise.resolve("nvidia/nemotron-3-ultra-550b-a55b");
        if (cmd === "list_conversations") return Promise.resolve(mockConversations);
        if (cmd === "get_conversation_messages") return Promise.resolve([]);
        if (cmd === "get_default_bot") {
          return Promise.resolve({
            id: "apex",
            name: "Apex",
            role: "Strategic Orchestrator",
            avatar: "zap",
            system_prompt: "",
            is_default: true,
          });
        }
        return Promise.resolve(null);
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Ask @/i);
      fireEvent.change(input, { target: { value: "Clean download folder and verify integrity" } });
      fireEvent.submit(input);

      await waitFor(() => {
        // Bug 5 contract: client template → "Suggested Workflow".
        expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
      });

      expect(capturedRunId).not.toBeNull();

      // Simulate step events: action step + verification step
      const stepCb = eventListeners["agent:step"];
      expect(stepCb).toBeDefined();

      // 1. Action tool execution
      stepCb({
        payload: {
          run_id: capturedRunId,
          step: 1,
          thought: "Moving files to target directories",
          tool_calls: [{ id: "tc_1", name: "execute_shell", arguments: {} }],
          tool_results: [{ tool_call_id: "tc_1", name: "execute_shell", output: "moved 3 files", is_error: false }],
          is_finished: false,
        },
      });

      // 2. Verification tool execution
      stepCb({
        payload: {
          run_id: capturedRunId,
          step: 2,
          thought: "Verifying file integrity and path structure",
          tool_calls: [{ id: "tc_2", name: "verify_file_integrity", arguments: {} }],
          tool_results: [{ tool_call_id: "tc_2", name: "verify_file_integrity", output: "checksum matches", is_error: false }],
          is_finished: true,
        },
      });

      // Finish agent turn
      runTurnPromiseResolve!("Organization complete and verified.");

      // Because verification actually executed, status is completed
      await waitFor(() => {
        expect(screen.getByText("completed")).toBeInTheDocument();
        expect(screen.getByText("Accomplished Actions:")).toBeInTheDocument();
        expect(screen.getByText(/Executed verify_file_integrity/i)).toBeInTheDocument();
      });
    });

    it("marks plan as failed when a tool execution fails", async () => {
      let turnResolve: (value: string) => void;
      const turnPromise = new Promise<string>((resolve) => {
        turnResolve = resolve;
      });

      let capturedRunId: string | null = null;
      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "run_agent_turn") {
          capturedRunId = args.runId;
          return turnPromise;
        }
        if (cmd === "get_selected_model") return Promise.resolve("nvidia/nemotron-3-ultra-550b-a55b");
        if (cmd === "list_conversations") return Promise.resolve(mockConversations);
        if (cmd === "get_conversation_messages") return Promise.resolve([]);
        if (cmd === "get_default_bot") {
          return Promise.resolve({
            id: "apex",
            name: "Apex",
            role: "Strategic Orchestrator",
            avatar: "zap",
            system_prompt: "",
            is_default: true,
          });
        }
        return Promise.resolve(null);
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
      });

      const input = screen.getByPlaceholderText(/Ask @/i);
      fireEvent.change(input, { target: { value: "Delete obsolete build artifacts" } });
      fireEvent.submit(input);

      await waitFor(() => {
        // Bug 5 contract: client template → "Suggested Workflow".
        expect(screen.getByText("Suggested Workflow")).toBeInTheDocument();
      });

      // Deliver step event with tool failure
      const stepCb = eventListeners["agent:step"];
      stepCb({
        payload: {
          run_id: capturedRunId,
          step: 1,
          thought: "Deleting directory",
          tool_calls: [{ id: "tc_err", name: "delete_directory", arguments: {} }],
          tool_results: [{ tool_call_id: "tc_err", name: "delete_directory", output: "Permission denied", is_error: true }],
          is_finished: true,
        },
      });

      turnResolve!("Encountered error during deletion.");

      await waitFor(() => {
        expect(screen.getByText("failed")).toBeInTheDocument();
        expect(screen.getByText(/Error in delete_directory: Permission denied/i)).toBeInTheDocument();
      });
    });
  });
});
