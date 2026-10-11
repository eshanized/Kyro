import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";

// Mock Tauri invoke and event listen APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("Recent Sessions - Session Deletion Lifecycle", () => {
  const mockConversations = [
    {
      id: "conv-1",
      title: "Task Alpha",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1700000000000,
    },
    {
      id: "conv-2",
      title: "Task Beta",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1700001000000,
    },
    {
      id: "conv-3",
      title: "Task Gamma",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1700002000000,
    },
  ];

  const mockMessagesMap: Record<string, any[]> = {
    "conv-1": [
      {
        id: "msg-1-1",
        conversation_id: "conv-1",
        role: "user",
        content: "First turn in Alpha",
        tool_calls_json: null,
        observations_json: null,
        created_at: 1700000010000,
      },
    ],
    "conv-2": [
      {
        id: "msg-2-1",
        conversation_id: "conv-2",
        role: "user",
        content: "First turn in Beta",
        tool_calls_json: null,
        observations_json: null,
        created_at: 1700001010000,
      },
    ],
    "conv-3": [
      {
        id: "msg-3-1",
        conversation_id: "conv-3",
        role: "user",
        content: "First turn in Gamma",
        tool_calls_json: null,
        observations_json: null,
        created_at: 1700002010000,
      },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    window.HTMLElement.prototype.scrollIntoView = vi.fn();

    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") {
        return [...mockConversations];
      }
      if (cmd === "get_messages") {
        return mockMessagesMap[args?.conversationId] || [];
      }
      if (cmd === "list_bots") {
        return [
          {
            id: "apex-orchestrator",
            name: "Apex",
            avatar: "zap",
            role: "Orchestrator",
            system_prompt: "",
            is_default: true,
          },
        ];
      }
      if (cmd === "get_takeover_status") {
        return { is_active: false, reason: "", timestamp: 0 };
      }
      if (cmd === "list_pending_approvals") {
        return [];
      }
      if (cmd === "delete_conversation") {
        return true;
      }
      return null;
    });
  });

  // 1. Every Recent Sessions entry exposes an accessible delete control
  it("renders an accessible delete control for every session entry in the sidebar", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
      expect(screen.getByText("Task Beta")).toBeInTheDocument();
      expect(screen.getByText("Task Gamma")).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByRole("button", { name: "Delete session" });
    expect(deleteButtons.length).toBe(mockConversations.length);

    deleteButtons.forEach((btn) => {
      expect(btn).toHaveAttribute("title", "Delete session");
      expect(btn).toHaveAttribute("aria-label", "Delete session");
    });
  });

  // 2. Clicking a delete control does not also select the session
  it("clicking the delete control stops propagation and does not select the session row", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
    });

    // Initial state: conv-1 (Task Alpha) is the first conversation, selected by default
    await waitFor(() => {
      expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
    });

    // Click delete on Task Beta (conv-2)
    const betaRow = screen.getByText("Task Beta").closest("div");
    const deleteBtn = within(betaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    // Confirmation dialog opens for Task Beta
    expect(screen.getByRole("heading", { name: "Delete session?" })).toBeInTheDocument();
    expect(screen.getByText(/Are you sure you want to delete “Task Beta”\?/i)).toBeInTheDocument();

    // Verify Task Alpha messages are STILL displayed (Task Beta was NOT activated)
    expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
    expect(screen.queryByText("First turn in Beta")).not.toBeInTheDocument();
  });

  // 3. The confirmation dialog displays the correct session title
  it("displays a confirmation dialog with the explicit title and consequences for the target session", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Beta")).toBeInTheDocument();
    });

    const betaRow = screen.getByText("Task Beta").closest("div");
    const deleteBtn = within(betaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Delete session?" })).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /Are you sure you want to delete “Task Beta”\? Its conversation messages will be permanently deleted\. This action cannot be undone\./i
      )
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Delete session" })).toBeInTheDocument();
  });

  // 4. Canceling preserves the session and does not invoke deletion
  it("canceling leaves all data unchanged without calling delete_conversation", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Beta")).toBeInTheDocument();
    });

    const betaRow = screen.getByText("Task Beta").closest("div");
    const deleteBtn = within(betaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    expect(screen.getByRole("dialog")).toBeInTheDocument();

    const dialog = screen.getByRole("dialog");
    const cancelBtn = within(dialog).getByRole("button", { name: "Cancel" });
    fireEvent.click(cancelBtn);

    // Dialog closes
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // delete_conversation was never invoked
    expect(invoke).not.toHaveBeenCalledWith("delete_conversation", expect.anything());

    // Task Beta is still present
    expect(screen.getByText("Task Beta")).toBeInTheDocument();
  });

  // 5. Confirming calls delete_conversation with the selected stable ID
  it("confirming calls delete_conversation with the selected session stable ID", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Beta")).toBeInTheDocument();
    });

    const betaRow = screen.getByText("Task Beta").closest("div");
    const deleteBtn = within(betaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_conversation", { id: "conv-2" });
    });
  });

  // 6. Successful deletion removes the session and its list entry
  it("removes the deleted session from the sidebar list after successful backend deletion", async () => {
    let currentConvs = [...mockConversations];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return currentConvs;
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        currentConvs = currentConvs.filter((c) => c.id !== args.id);
        return true;
      }
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Beta")).toBeInTheDocument();
    });

    const betaRow = screen.getByText("Task Beta").closest("div");
    const deleteBtn = within(betaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.queryByText("Task Beta")).not.toBeInTheDocument();
    });

    // Unrelated sessions remain in list
    expect(screen.getByText("Task Alpha")).toBeInTheDocument();
    expect(screen.getByText("Task Gamma")).toBeInTheDocument();
  });

  // 7. Deleting the active session selects a valid remaining session or clears workspace when empty
  it("selects another surviving session when the active session is deleted", async () => {
    let currentConvs = [...mockConversations];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return currentConvs;
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        currentConvs = currentConvs.filter((c) => c.id !== args.id);
        return true;
      }
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
      expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
    });

    // Delete active session (conv-1: Task Alpha)
    const alphaRow = screen.getByText("Task Alpha").closest("div");
    const deleteBtn = within(alphaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    // Surviving session Task Beta (conv-2) becomes active and loads its messages
    await waitFor(() => {
      expect(screen.queryByText("Task Alpha")).not.toBeInTheDocument();
      expect(screen.getByText("First turn in Beta")).toBeInTheDocument();
    });
  });

  it("clears workspace when the last session is deleted and no sessions remain", async () => {
    let currentConvs = [mockConversations[0]]; // Only Task Alpha
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return currentConvs;
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        currentConvs = [];
        return true;
      }
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
      expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
    });

    // Delete the only session
    const alphaRow = screen.getByText("Task Alpha").closest("div");
    const deleteBtn = within(alphaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.queryByText("Task Alpha")).not.toBeInTheDocument();
      expect(screen.queryByText("First turn in Alpha")).not.toBeInTheDocument();
    });
  });

  // 8. Deleting a non-active session preserves the current selection
  it("deleting a non-active session preserves the current active session and its content", async () => {
    let currentConvs = [...mockConversations];
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return currentConvs;
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        currentConvs = currentConvs.filter((c) => c.id !== args.id);
        return true;
      }
      return null;
    });

    render(<App />);

    // conv-1 (Task Alpha) is active
    await waitFor(() => {
      expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
    });

    // Delete non-active session conv-3 (Task Gamma)
    const gammaRow = screen.getByText("Task Gamma").closest("div");
    const deleteBtn = within(gammaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.queryByText("Task Gamma")).not.toBeInTheDocument();
    });

    // Task Alpha remains active and its messages remain visible
    expect(screen.getByText("First turn in Alpha")).toBeInTheDocument();
  });

  // 9. Backend failure keeps the UI consistent and displays an actionable error
  it("keeps dialog open and displays actionable error when backend rejects deletion", async () => {
    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return [...mockConversations];
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        throw new Error(
          "Cannot delete session: conversation 'conv-1' has an active task in progress. The task must finish or be stopped before the session can be deleted."
        );
      }
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
    });

    const alphaRow = screen.getByText("Task Alpha").closest("div");
    const deleteBtn = within(alphaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    fireEvent.click(confirmBtn);

    // Dialog stays open and shows error
    await waitFor(() => {
      expect(
        screen.getByText(
          /Cannot delete session: conversation 'conv-1' has an active task in progress\. The task must finish or be stopped before the session can be deleted\./i
        )
      ).toBeInTheDocument();
    });

    // Session is still present in the list
    expect(screen.getByText("Task Alpha")).toBeInTheDocument();
  });

  // 10. Duplicate deletion submissions are prevented
  it("disables deletion buttons during in-flight deletion preventing duplicate requests", async () => {
    let resolveDeletePromise: (val: boolean) => void = () => {};
    const deletePromise = new Promise<boolean>((resolve) => {
      resolveDeletePromise = resolve;
    });

    (invoke as any).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_conversations") return [...mockConversations];
      if (cmd === "get_messages") return mockMessagesMap[args?.conversationId] || [];
      if (cmd === "list_bots") return [{ id: "apex", name: "Apex", avatar: "zap", role: "Orchestrator", system_prompt: "", is_default: true }];
      if (cmd === "get_takeover_status") return { is_active: false, reason: "", timestamp: 0 };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "delete_conversation") {
        return deletePromise;
      }
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task Alpha")).toBeInTheDocument();
    });

    const alphaRow = screen.getByText("Task Alpha").closest("div");
    const deleteBtn = within(alphaRow!).getByRole("button", { name: "Delete session" });
    fireEvent.click(deleteBtn);

    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete session" });
    const cancelBtn = within(dialog).getByRole("button", { name: "Cancel" });

    // Click confirm
    fireEvent.click(confirmBtn);

    // While deletion is in-flight, buttons are disabled
    await waitFor(() => {
      expect(confirmBtn).toBeDisabled();
      expect(confirmBtn).toHaveTextContent("Deleting...");
      expect(cancelBtn).toBeDisabled();
    });

    // Second click attempt should be blocked
    fireEvent.click(confirmBtn);
    const deleteCalls = (invoke as any).mock.calls.filter(
      (c: any[]) => c[0] === "delete_conversation"
    );
    expect(deleteCalls.length).toBe(1);

    // Complete the promise
    resolveDeletePromise(true);

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
