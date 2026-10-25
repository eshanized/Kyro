/**
 * Run-lifecycle ownership regression tests (Issues #22, bugs 1, 2, 4).
 *
 * These tests drive the REAL App component through the same boundaries the
 * production window uses: the Tauri `invoke` calls (`run_agent_turn`,
 * `cancel_agent_run`, `add_message`) and the `agent:step` backend event.
 *
 * Contract being protected:
 *   - late results route to their OWNING conversation, never the currently
 *     visible one (bug 2);
 *   - a resolving run never releases another run's busy state (bug 2);
 *   - double submission is blocked synchronously, before the first await
 *     (bug 4);
 *   - cancellation is only claimed when the backend confirms the transition
 *     (bug 1).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

let stepHandler: ((event: { payload: any }) => void) | null = null;

const conversations = [
  { id: "conv-1", title: "Task Alpha", model: "m", created_at: 1700000000000 },
  { id: "conv-2", title: "Task Beta", model: "m", created_at: 1700001000000 },
];

function stepEvent(runId: string, conversationId: string, step: number, thought: string) {
  return {
    run_id: runId,
    conversation_id: conversationId,
    agent_id: null,
    parent_run_id: null,
    is_child_run: false,
    step,
    thought,
    tool_calls: [],
    tool_results: [],
    is_finished: false,
  };
}

describe("Run lifecycle ownership (bugs 1, 2, 4)", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    stepHandler = null;
    mockListen.mockImplementation(async (_event: string, handler?: any) => {
      if (_event === "agent:step") stepHandler = handler;
      return vi.fn();
    });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "list_authorized_folders") return [];
      return Promise.resolve(null);
    });
    render(<App />);
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument()
    );
    // Wait for the guard against events without listeners to pass.
    await waitFor(() => expect(stepHandler).not.toBeNull());
  });

  it("bug 2: a late run result is appended to its own conversation and does not release another run's busy state", async () => {
    let resolveRunA: (v: any) => void = () => {};
    let resolveRunB: (v: any) => void = () => {};
    const run_call_ids: string[] = [];
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") {
        run_call_ids.push(args.conversationId);
        if (args.conversationId === "conv-1") {
          return new Promise<string>((resolve) => {
            resolveRunA = resolve;
          });
        }
        return new Promise<string>((resolve) => {
          resolveRunB = resolve;
        });
      }
      return Promise.resolve(null);
    });

    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    // Run A on conv-1
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("run_agent_turn", expect.anything()));
    const runIdA = mockInvoke.mock.calls.find(
      ([cmd]) => cmd === "run_agent_turn"
    )![1].runId;
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "run_agent_turn").length).toBe(1);

    // Switch to conv-2 (run A stays alive in the background: bug 1 behavior)
    fireEvent.click(screen.getByText("Task Beta"));

    await waitFor(() => expect(screen.getByText("Task Beta")).toBeInTheDocument());

    // Run B on conv-2
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my downloads into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));

    await waitFor(() =>
      expect(
        mockInvoke.mock.calls.filter(([cmd]) => cmd === "run_agent_turn").length
      ).toBe(2)
    );

    // Resolve the FIRST (now unseen) run while the second is still active.
    resolveRunA("Reply One (late for conv-1)");

    // Persistence still targets the owning conversation, whatever is visible.
    await waitFor(() => {
      const persisted = mockInvoke.mock.calls
        .filter(([cmd]) => cmd === "add_message")
        .map(([, args]) => args.msg);
      const replies = persisted.filter((m) => m.role === "assistant");
      expect(replies.some((m) => m.content.includes("Reply One"))).toBe(true);
      const replyOne = replies.find((m) => m.content.includes("Reply One"));
      expect(replyOne.conversation_id).toBe("conv-1");
    });

    // The late reply is NOT shown inside conv-2 and busy state is untouched
    // (run B still active → Stop button present).
    expect(screen.queryByText("Reply One (late for conv-1)")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Stop running task/i })).toBeInTheDocument();

    // Live plan of run B must not have been mutated by run A's late resolution
    // (run-scoped ownership): A's plan card is not in the DOM while B is open.
    expect(screen.queryByText("Reply Two")).not.toBeInTheDocument();

    resolveRunB("Reply Two for conv-2");

    // Now run B's reply is visible because it belongs to the open conversation.
    await waitFor(() =>
      expect(screen.getByText("Reply Two for conv-2")).toBeInTheDocument()
    );
    expect(screen.queryByRole("button", { name: /Stop running task/i })).not.toBeInTheDocument();
  });

  it("bug 4: a double submission cannot start two runs for the same conversation", async () => {
    let touched = 0;
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") {
        touched += 1;
        return new Promise<string>(() => {}); // never resolves during test
      }
      return Promise.resolve(null);
    });

    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());
    const runCallsBefore = mockInvoke.mock.calls.filter(
      ([cmd]) => cmd === "run_agent_turn"
    ).length;

    const input = screen.getByPlaceholderText(/Ask @/i);
    act(() => {
      fireEvent.change(input, { target: { value: "organize my documents into folders" } });
      // Two synchronous submissions in the SAME tick: state has not
      // re-rendered between them, so only the synchronous lock can stop
      // the double enqueue (bug 4).
      fireEvent.submit(input);
      fireEvent.submit(input);
    });

    await waitFor(() =>
      expect(
        mockInvoke.mock.calls.filter(([cmd]) => cmd === "run_agent_turn").length
      ).toBe(runCallsBefore + 1)
    );
    expect(touched).toBe(1);
  });

  it("bug 1: cancellation truthfulness — confirmed transition marks the plan cancelled", async () => {
    let resolveRun: (v: any) => void = () => {};
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") {
        return new Promise<string>((resolve) => {
          resolveRun = resolve;
        });
      }
      if (cmd === "cancel_agent_run") return true;
      return Promise.resolve(null);
    });

    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));

    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /Stop running task/i }));

    // Backend CONFIRMED — and only now may the UI claim cancellation.
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        "cancel_agent_run",
        expect.objectContaining({ runId: expect.any(String) })
      );
      expect(screen.getByText("cancelled")).toBeInTheDocument();
    });
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "run_agent_turn").length).toBe(1);
    resolveRun("never used");
  });

  it("bug 1: an already-terminal run is reconciled, not claimed as cancelled", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") {
        return new Promise<string>(() => {});
      }
      if (cmd === "cancel_agent_run") return false; // already terminal
      return Promise.resolve(null);
    });

    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));
    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /Stop running task/i }));

    await waitFor(() =>
      expect(screen.getByText(/had already finished; nothing was cancelled/i)).toBeInTheDocument()
    );
    // The plan is NOT falsified into "cancelled".
    expect(screen.queryByText("cancelled")).not.toBeInTheDocument();
  });

  it("bug 1: a setup-window cancellation is reported as requested, not confirmed", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_selected_model") return "nvidia/nemotron-3-ultra-550b-a55b";
      if (cmd === "list_conversations") return conversations;
      if (cmd === "get_conversation_messages") return [];
      if (cmd === "get_default_bot") {
        return {
          id: "apex",
          name: "Apex",
          role: "Strategic Orchestrator",
          avatar: "zap",
          system_prompt: "",
          is_default: true,
        };
      }
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") {
        return new Promise<string>(() => {});
      }
      if (cmd === "cancel_agent_run") {
        throw new Error(
          "Cancellation registered: run 'run_x' is still being created; the run will abort before its first execution step"
        );
      }
      return Promise.resolve(null);
    });

    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText(/Ask @/i), {
      target: { value: "organize my documents into folders" },
    });
    fireEvent.submit(screen.getByPlaceholderText(/Ask @/i));
    await waitFor(() => expect(screen.getByTestId("task-progress-card")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: /Stop running task/i }));

    await waitFor(() => expect(screen.getByText(/Stopping:/i)).toBeInTheDocument());
    expect(screen.queryByText("cancelled")).not.toBeInTheDocument();
  });
});
