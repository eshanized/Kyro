import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
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

/** The handler that the App registered for the "agent:step" event. */
let stepHandler: ((event: { payload: any }) => void) | null = null;

const conversations = [
  { id: "conv-1", title: "Task Alpha", model: "m", created_at: 1700000000000 },
  { id: "conv-2", title: "Task Beta", model: "m", created_at: 1700001000000 },
];

function stepEvent(runId: string, conversationId: string, step: number, thought: string) {
  return {
    run_id: runId,
    conversation_id: conversationId,
    agent_id: "apex-orchestrator",
    parent_run_id: null,
    is_child_run: false,
    step,
    thought,
    tool_calls: [],
    tool_results: [],
    is_finished: false,
  };
}

describe("Agent step event attribution", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stepHandler = null;

    mockListen.mockImplementation((event: string, handler: any) => {
      if (event === "agent:step") {
        stepHandler = handler;
      }
      return Promise.resolve(() => {}) as any;
    });

    mockInvoke.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "list_conversations":
          return conversations;
        case "get_messages":
          return [];
        case "list_bots":
          return [];
        case "get_takeover_status":
          return { is_active: false, reason: "", timestamp: 0 };
        case "list_pending_approvals":
          return [];
        case "get_roster_statuses":
          return [];
        case "get_installed_apps":
          return [];
        case "list_memories":
          return [];
        case "get_system_health":
          return {
            db_connected: true,
            db_wal_mode: true,
            docker_available: false,
            docker_message: "",
            ghost_xvfb_available: false,
            os_keyring_available: false,
            display_server: "X11",
            tools: {},
          };
        default:
          return null;
      }
    });
  });

  it("registers a listener for the agent step event", async () => {
    render(<App />);
    await waitFor(() => expect(mockListen).toHaveBeenCalled());
    expect(mockListen.mock.calls.some((call) => call[0] === "agent:step")).toBe(true);
  });

  it("ignores an event that does not belong to the open conversation", async () => {
    render(<App />);
    await waitFor(() => expect(stepHandler).not.toBeNull());

    // The app opens the first conversation, "conv-1".
    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    act(() => {
      stepHandler?.({
        payload: stepEvent("run-b", "conv-2", 1, "Event from another conversation"),
      });
    });

    expect(screen.queryByText("Event from another conversation")).toBeNull();
  });

  it("shows an event that belongs to the open conversation", async () => {
    render(<App />);
    await waitFor(() => expect(stepHandler).not.toBeNull());
    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    act(() => {
      stepHandler?.({
        payload: stepEvent("run-a", "conv-1", 1, "Event for this conversation"),
      });
    });

    await waitFor(() =>
      expect(screen.getByText("Event for this conversation")).toBeInTheDocument()
    );
  });

  it("keeps the events of concurrent runs apart", async () => {
    render(<App />);
    await waitFor(() => expect(stepHandler).not.toBeNull());
    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    act(() => {
      stepHandler?.({ payload: stepEvent("run-a", "conv-1", 1, "First run step") });
      stepHandler?.({ payload: stepEvent("run-a", "conv-1", 2, "First run step two") });
    });

    await waitFor(() => expect(screen.getByText("First run step")).toBeInTheDocument());
    expect(screen.getByText("First run step two")).toBeInTheDocument();

    // A second run in the same conversation replaces the visible timeline,
    // but the first run's events are not lost from the store.
    act(() => {
      stepHandler?.({ payload: stepEvent("run-b", "conv-1", 1, "Second run step") });
    });

    await waitFor(() => expect(screen.getByText("Second run step")).toBeInTheDocument());
  });

  it("ignores an event without a run identity", async () => {
    render(<App />);
    await waitFor(() => expect(stepHandler).not.toBeNull());
    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    act(() => {
      stepHandler?.({
        payload: {
          step: 1,
          thought: "Legacy event without identity",
          tool_calls: [],
          tool_results: [],
          is_finished: false,
        },
      });
    });

    expect(screen.queryByText("Legacy event without identity")).toBeNull();
  });

  it("marks a child agent's output", async () => {
    render(<App />);
    await waitFor(() => expect(stepHandler).not.toBeNull());
    await waitFor(() => expect(screen.getByText("Task Alpha")).toBeInTheDocument());

    act(() => {
      stepHandler?.({
        payload: {
          ...stepEvent("run-child", "conv-1", 1, "Delegated output"),
          parent_run_id: "run-a",
          is_child_run: true,
          agent_id: "forge-devops",
        },
      });
    });

    await waitFor(() => expect(screen.getByText("Delegated output")).toBeInTheDocument());
    expect(screen.getByText(/Delegated agent/)).toBeInTheDocument();
  });
});
