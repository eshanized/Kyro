import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class MockTerminal {
      loadAddon = vi.fn();
      open = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      onData = vi.fn();
      dispose = vi.fn();
    },
  };
});

vi.mock("@xterm/addon-fit", () => {
  return {
    FitAddon: class MockFitAddon {
      fit = vi.fn();
    },
  };
});

describe("App Comprehensive Integration Tests", () => {
  const mockConversations = [
    {
      id: "conv_1",
      title: "First Chat Session",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1000,
      updated_at: 1000,
    },
    {
      id: "conv_2",
      title: "Second Project Task",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 2000,
      updated_at: 2000,
    },
  ];

  const mockMessages = [
    {
      id: "msg_1",
      conversation_id: "conv_1",
      role: "user",
      content: "Hello Kyro, help organize my files",
      created_at: 1000,
    },
    {
      id: "msg_2",
      conversation_id: "conv_1",
      role: "assistant",
      content: "I can help with that. What directory should I scan?",
      created_at: 1001,
    },
  ];

  const mockBots = [
    {
      id: "apex",
      name: "Apex",
      avatar: "zap",
      role: "General Workspace & Strategic Orchestrator",
      system_prompt: "Coordinate tasks",
      is_default: true,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    // Mark onboarding completed so main view is visible directly
    localStorage.setItem("kyro_onboarding_completed", "true");

    (invoke as any).mockImplementation((cmd: string, args: any) => {
      if (cmd === "list_conversations") {
        return Promise.resolve(mockConversations);
      }
      if (cmd === "get_messages") {
        return Promise.resolve(mockMessages);
      }
      if (cmd === "list_bots") {
        return Promise.resolve(mockBots);
      }
      if (cmd === "get_takeover_status") {
        return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
      }
      if (cmd === "list_pending_approvals") {
        return Promise.resolve([]);
      }
      if (cmd === "create_conversation") {
        return Promise.resolve({
          id: "conv_new",
          title: "New Chat",
          model: "nvidia/nemotron-3-ultra-550b-a55b",
          created_at: 3000,
          updated_at: 3000,
        });
      }
      if (cmd === "delete_conversation") {
        return Promise.resolve(true);
      }
      if (cmd === "run_agent_turn") {
        return Promise.resolve({
          run_id: "run_test_turn",
          response: "Task started",
        });
      }
      if (cmd === "get_suggested_folders") {
        return Promise.resolve([]);
      }
      if (cmd === "list_connector_catalog") {
        return Promise.resolve([]);
      }
      if (cmd === "list_connected_accounts") {
        return Promise.resolve([]);
      }
      if (cmd === "list_mcp_servers") {
        return Promise.resolve([]);
      }
      if (cmd === "list_artifacts") {
        return Promise.resolve([]);
      }
      if (cmd === "get_timeline_steps") {
        return Promise.resolve([]);
      }
      if (cmd === "list_routines") {
        return Promise.resolve([]);
      }
      if (cmd === "list_routine_runs") {
        return Promise.resolve([]);
      }
      if (cmd === "get_calendar_events") {
        return Promise.resolve([]);
      }
      if (cmd === "get_api_keys") {
        return Promise.resolve({
          nvidia_configured: false,
          telegram_configured: false,
          telegram_chat_id: "",
        });
      }
      if (cmd === "list_skills") {
        return Promise.resolve([]);
      }
      if (cmd === "get_sandbox_status") {
        return Promise.resolve({ supported: false, active: false });
      }
      if (cmd === "get_browser_capabilities") {
        return Promise.resolve({ available: false });
      }
      if (cmd === "get_browser_settings") {
        return Promise.resolve({});
      }
      if (cmd === "list_authorized_folders") {
        return Promise.resolve([]);
      }
      if (cmd === "toggle_ghost_mode") {
        return Promise.resolve(true);
      }
      return Promise.resolve(null);
    });
  });

  it("renders main App shell, conversations list, and message history", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    expect(screen.getByText("Second Project Task")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText("Hello Kyro, help organize my files")).toBeInTheDocument();
      expect(screen.getByText("I can help with that. What directory should I scan?")).toBeInTheDocument();
    });
  });

  it("switches conversation session on sidebar click", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Second Project Task")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Second Project Task"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_messages", { conversationId: "conv_2" });
    });
  });

  it("toggles experience mode between Everyday and Dev Mode", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    // Check toggle button
    const devModeToggle = screen.getByRole("button", { name: /Dev Mode/i });
    fireEvent.click(devModeToggle);

    expect(localStorage.getItem("kyro_experience_mode")).toBe("dev");

    // Toggle back to Everyday
    const everydayToggle = screen.getByRole("button", { name: /Everyday/i });
    fireEvent.click(everydayToggle);

    expect(localStorage.getItem("kyro_experience_mode")).toBe("everyday");
  });

  it("switches everyday view destinations (Ask, Today, Tools, Routines)", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    // Switch to Today tab
    const todayBtns = screen.getAllByRole("button", { name: /Today/i });
    fireEvent.click(todayBtns[0]);

    // Switch to Tools tab
    const toolsBtns = screen.getAllByRole("button", { name: /Tools/i });
    fireEvent.click(toolsBtns[0]);

    // Switch to Routines tab
    const routinesBtns = screen.getAllByRole("button", { name: /Routines/i });
    fireEvent.click(routinesBtns[0]);

    // Switch back to Ask tab
    const askBtns = screen.getAllByRole("button", { name: /Ask Kyro/i });
    fireEvent.click(askBtns[0]);

    await waitFor(() => {
      expect(screen.getByText("Hello Kyro, help organize my files")).toBeInTheDocument();
    });
  });

  it("submits prompt message and creates new turn", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    const textarea = await screen.findByPlaceholderText(/Ask @Apex/i);
    fireEvent.change(textarea, { target: { value: "Review codebase architecture" } });
    expect(textarea).toHaveValue("Review codebase architecture");

    const sendBtn = screen.getByRole("button", { name: /Send/i });
    fireEvent.click(sendBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "run_agent_turn",
        expect.objectContaining({
          conversationId: "conv_1",
          history: expect.arrayContaining([
            expect.objectContaining({
              content: "Review codebase architecture",
              role: "user",
            }),
          ]),
        })
      );
    });
  });

  it("opens and cancels session deletion dialog", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    // Find delete session button by test id
    const deleteBtn = screen.getByTestId("delete-session-button-conv_1");
    fireEvent.click(deleteBtn);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText(/Are you sure you want to delete/i)).toBeInTheDocument();

    // Cancel deletion within the dialog
    const cancelBtn = within(dialog).getByRole("button", { name: /Cancel deletion/i });
    fireEvent.click(cancelBtn);

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("handles CommandPalette keyboard shortcut (Ctrl+K)", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    // Trigger Ctrl+K
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Type a command or search/i)).toBeInTheDocument();
    });

    // Close with Escape
    fireEvent.keyDown(window, { key: "Escape" });
  });

  it("renders pending approval requests and handles approval click", async () => {
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_conversations") return Promise.resolve(mockConversations);
      if (cmd === "get_messages") return Promise.resolve(mockMessages);
      if (cmd === "list_bots") return Promise.resolve(mockBots);
      if (cmd === "get_takeover_status") return Promise.resolve({ is_active: false, reason: "", timestamp: 0 });
      if (cmd === "list_pending_approvals") {
        return Promise.resolve([
          {
            id: "appr_ui_1",
            run_id: "run_active",
            tool_name: "execute_shell",
            summary: "Run destructive script",
            args_json: JSON.stringify({ command: "rm -rf /tmp/cache" }),
            risk_level: "destructive",
            reason: "Cache cleanup",
            status: "pending",
            created_at: 1000,
            expires_at: 9999999999,
          },
        ]);
      }
      if (cmd === "resolve_approval") {
        return Promise.resolve(true);
      }
      return Promise.resolve(null);
    });

    render(<App />);

    await waitFor(
      () => {
        expect(screen.getByText(/Run destructive script/i)).toBeInTheDocument();
      },
      { timeout: 3500 }
    );

    // Click Approve button
    const approveBtn = screen.getByRole("button", { name: /Approve/i });
    fireEvent.click(approveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("resolve_approval", {
        id: "appr_ui_1",
        decision: "approved",
        resolvedBy: "desktop_user",
      });
    });
  });

  it("creates a new chat session when clicking New Session button", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    const newSessionBtn = screen.getByRole("button", { name: /New Session/i });
    fireEvent.click(newSessionBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_conversation",
        expect.objectContaining({
          id: expect.stringMatching(/^conv_/),
          title: "New Task 3",
        })
      );
    });
  });

  it("confirms deletion of a conversation session", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    const deleteBtn = screen.getByTestId("delete-session-button-conv_1");
    fireEvent.click(deleteBtn);

    const confirmBtn = await screen.findByTestId("confirm-delete-session-button");
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_conversation", { id: "conv_1" });
    });
  });

  it("clicks quick prompt pill to populate prompt input", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    const pill = screen.getByText("Find the invoice I downloaded last month");
    fireEvent.click(pill);

    const textarea = screen.getByPlaceholderText(/Ask @Apex/i);
    expect(textarea).toHaveValue("Find the invoice I downloaded last month");
  });

  it("handles active takeover and resumes autonomous agent", async () => {
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_conversations") return Promise.resolve(mockConversations);
      if (cmd === "get_messages") return Promise.resolve(mockMessages);
      if (cmd === "list_bots") return Promise.resolve(mockBots);
      if (cmd === "get_takeover_status") {
        return Promise.resolve({
          is_active: true,
          reason: "Autonomous execution paused for manual verification.",
          timestamp: 5000,
        });
      }
      if (cmd === "list_pending_approvals") return Promise.resolve([]);
      if (cmd === "resume_takeover") return Promise.resolve(true);
      return Promise.resolve(null);
    });

    render(<App />);

    await waitFor(
      () => {
        expect(screen.getByText(/Human Takeover Active/i)).toBeInTheDocument();
      },
      { timeout: 3500 }
    );

    const resumeBtn = screen.getByRole("button", { name: /Resume Autonomous Agent/i });
    fireEvent.click(resumeBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("resume_takeover");
    });
  });

  it("opens Settings modal, Dev modals, and MCP/Connections/Teammates/Artifacts modals", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("First Chat Session")).toBeInTheDocument();
    });

    // Open Settings modal
    const settingsBtn = screen.getByRole("button", { name: /Vault & Settings/i });
    fireEvent.click(settingsBtn);

    // Switch to Dev Mode to access dev actions
    const devModeToggle = screen.getByRole("button", { name: /Dev Mode/i });
    fireEvent.click(devModeToggle);

    // Click App Connections
    const connBtn = screen.getByRole("button", { name: /App Connections/i });
    fireEvent.click(connBtn);

    // Click Artifacts Registry
    const artBtn = screen.getByRole("button", { name: /Artifacts Registry/i });
    fireEvent.click(artBtn);

    // Click Teammate Roster
    const teamBtn = screen.getByRole("button", { name: /Teammate Roster/i });
    fireEvent.click(teamBtn);

    // Click MCP Tool Servers
    const mcpBtn = screen.getByRole("button", { name: /MCP Tool Servers/i });
    fireEvent.click(mcpBtn);

    // Click Teach-a-Task & Routines
    const teachBtn = screen.getByRole("button", { name: /Teach-a-Task & Routines/i });
    fireEvent.click(teachBtn);

    // Toggle Ghost mode
    const ghostBtn = screen.getByRole("button", { name: /Ghost/i });
    fireEvent.click(ghostBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("toggle_ghost_mode", { enabled: true });
    });

    // Toggle PTY terminal
    const ptyBtn = screen.getByRole("button", { name: /PTY/i });
    fireEvent.click(ptyBtn);

    expect(screen.getByText(/Host Terminal PTY/i)).toBeInTheDocument();
  });
});
