import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { McpModal } from "../components/McpModal";
import { CommandPalette } from "../components/CommandPalette";
import { TodayDashboardView } from "../components/TodayDashboardView";
import { TeammateRosterModal } from "../components/TeammateRosterModal";
import { ArtifactsModal } from "../components/ArtifactsModal";
import { ScreenExplainerModal } from "../components/ScreenExplainerModal";
import { OnboardingModal } from "../components/OnboardingModal";
import { TimelineDrawer } from "../components/TimelineDrawer";
import { TerminalView } from "../components/TerminalView";
import { App } from "../App";
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

vi.mock("@xterm/xterm", () => {
  return {
    Terminal: class MockTerminal {
      rows = 24;
      cols = 80;
      loadAddon = vi.fn();
      open = vi.fn();
      writeln = vi.fn();
      write = vi.fn();
      onData = vi.fn((cb) => {
        (this as any)._onDataCb = cb;
      });
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

describe("McpModal Integration Tests", () => {
  const mockMcpServers = [
    {
      id: "mcp_1",
      name: "Local Filesystem",
      command: "npx",
      args_json: JSON.stringify(["-y", "@modelcontextprotocol/server-filesystem", "/home/snigdha"]),
      env_json: "{}",
      enabled: true,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_mcp_servers") return Promise.resolve(mockMcpServers);
      if (cmd === "create_mcp_server") return Promise.resolve(mockMcpServers[0]);
      if (cmd === "delete_mcp_server") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("lists servers, opens add form, and creates new MCP server", async () => {
    render(<McpModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Local Filesystem")).toBeInTheDocument();
    });

    const addBtn = screen.getByRole("button", { name: /Add MCP Server/i });
    fireEvent.click(addBtn);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Local Filesystem or Brave Search/i);
    fireEvent.change(nameInput, { target: { value: "PostgreSQL Connector" } });

    const saveBtn = screen.getByRole("button", { name: /Save & Connect Server/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_mcp_server",
        expect.objectContaining({
          server: expect.objectContaining({
            name: "PostgreSQL Connector",
          }),
        })
      );
    });
  });

  it("opens delete confirmation dialog and deletes MCP server", async () => {
    render(<McpModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Local Filesystem")).toBeInTheDocument();
    });

    const deleteBtn = screen.getByRole("button", { name: /Delete Local Filesystem/i });
    fireEvent.click(deleteBtn);

    const confirmBtn = screen.getByRole("button", { name: /^Remove server$/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_mcp_server", { id: "mcp_1" });
    });
  });
});

describe("CommandPalette Search & Action Execution", () => {
  const mockSearchResults = [
    {
      id: "conv_inv_101",
      category: "conversation",
      title: "Invoice processing automation",
      snippet: "Extract tax IDs from March invoices",
      destination_id: "conv_inv_101",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "search_workspace") return Promise.resolve(mockSearchResults);
      return Promise.resolve(null);
    });
  });

  it("searches workspace, navigates with keyboard, and activates conversation", async () => {
    const handleSelectConversation = vi.fn();
    const handleClose = vi.fn();

    render(
      <CommandPalette
        isOpen={true}
        onClose={handleClose}
        onNewSession={vi.fn()}
        onToggleGhost={vi.fn()}
        onToggleTerminal={vi.fn()}
        onOpenTeammates={vi.fn()}
        onOpenTeachTask={vi.fn()}
        onOpenMcp={vi.fn()}
        onOpenTimeline={vi.fn()}
        onOpenArtifacts={vi.fn()}
        onOpenSettings={vi.fn()}
        onSelectConversation={handleSelectConversation}
        onSelectBot={vi.fn()}
      />
    );

    const input = screen.getByPlaceholderText(/Type a command or search/i);
    fireEvent.change(input, { target: { value: "Invoice" } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search_workspace", { query: "Invoice" });
    });

    await waitFor(() => {
      expect(screen.getByText("Invoice processing automation")).toBeInTheDocument();
    });

    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(handleSelectConversation).toHaveBeenCalledWith("conv_inv_101");
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it("handles Escape key to close modal", () => {
    const handleClose = vi.fn();

    render(
      <CommandPalette
        isOpen={true}
        onClose={handleClose}
        onNewSession={vi.fn()}
        onToggleGhost={vi.fn()}
        onToggleTerminal={vi.fn()}
        onOpenTeammates={vi.fn()}
        onOpenTeachTask={vi.fn()}
        onOpenMcp={vi.fn()}
        onOpenTimeline={vi.fn()}
        onOpenArtifacts={vi.fn()}
        onOpenSettings={vi.fn()}
        onSelectConversation={vi.fn()}
        onSelectBot={vi.fn()}
      />
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(handleClose).toHaveBeenCalledTimes(1);
  });
});

describe("TodayDashboardView Rejection & Quick Actions", () => {
  const mockTodayData = {
    greeting: "Good afternoon, Engineer",
    today_date: "Thursday, October 22, 2026",
    pending_approvals: [
      {
        id: "appr_reject_1",
        tool_name: "write_restricted_system_file",
        summary: "Write /etc/hosts entry",
        reason: "DNS redirect for dev testing",
        created_at: 1000,
      },
    ],
    upcoming_routines: [],
    recent_runs: [],
    quick_stats: {
      completed_tasks_today: 3,
      active_connections: 2,
      saved_hours_estimate: 1.0,
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "get_today_dashboard_data") return Promise.resolve(mockTodayData);
      if (cmd === "resolve_approval") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("rejects pending approval with negative decision outcome", async () => {
    render(
      <TodayDashboardView
        onOpenTool={vi.fn()}
        onAskPrompt={vi.fn()}
        onOpenConnections={vi.fn()}
        onOpenSettings={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("write_restricted_system_file")).toBeInTheDocument();
    });

    const rejectBtn = screen.getByRole("button", { name: /Reject/i });
    fireEvent.click(rejectBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "resolve_approval",
        expect.objectContaining({
          id: "appr_reject_1",
          decision: "denied",
          resolvedBy: "today_dashboard",
        })
      );
    });
  });

  it("triggers quick actions prompts from dashboard cards", async () => {
    const handleAskPrompt = vi.fn();
    const handleOpenTool = vi.fn();

    render(
      <TodayDashboardView
        onOpenTool={handleOpenTool}
        onAskPrompt={handleAskPrompt}
        onOpenConnections={vi.fn()}
        onOpenSettings={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Organize Downloaded Files")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Organize Downloaded Files"));
    expect(handleAskPrompt).toHaveBeenCalledWith(
      expect.stringContaining("Organize my college documents into appropriate folders")
    );
  });
});

describe("TeammateRosterModal Creation & Deletion Flow", () => {
  const mockProfile = {
    id: "sentinel-reviewer",
    name: "Sentinel",
    avatar: "shield",
    role: "Static analysis & review",
    description: "Reviews changes",
    model_id: null,
    origin: "custom",
    enabled: true,
    runtime_disabled: false,
    schema_version: 1,
    version_label: "v1:def456",
    aliases: [],
    validation_error: null,
    source_path: "/tmp/agents/custom/sentinel-reviewer.md",
    instructions: "You are Sentinel.",
    tools_mode: "inherit",
    tools_allow: [],
    delegation_enabled: true,
    delegation_max_depth: 3,
    delegation_max_children: 4,
    max_steps: 25,
    is_builtin: false,
  };

  const mockStatuses = [
    {
      id: "sentinel-reviewer",
      state: "busy",
      current_task: "Compiling Architecture.md",
      active_tasks: [{ run_id: "run_1", objective: "Compiling Architecture.md" }],
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string, _args?: any) => {
      if (cmd === "list_agent_profiles") return Promise.resolve([mockProfile]);
      if (cmd === "get_roster_statuses") return Promise.resolve(mockStatuses);
      if (cmd === "create_agent_profile") return Promise.resolve(mockProfile);
      if (cmd === "delete_agent_profile") return Promise.resolve("custom");
      if (cmd === "set_agent_profile_enabled") return Promise.resolve();
      return Promise.resolve(null);
    });
  });

  it("creates a new specialist profile and displays busy status", async () => {
    render(
      <TeammateRosterModal
        isOpen={true}
        onClose={vi.fn()}
        selectedBotId="sentinel-reviewer"
        onSelectBot={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Compiling Architecture\.md/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /Add Teammate/i }));

    const idInput = screen.getByPlaceholderText(/sentinel-reviewer/i);
    const nameInput = screen.getByPlaceholderText(/^Sentinel$/i);
    const roleInput = screen.getByPlaceholderText(/Role \(e\.g\. Code Review & Static Analysis Specialist\)/i);
    fireEvent.change(idInput, { target: { value: "sentinel-reviewer" } });
    fireEvent.change(nameInput, { target: { value: "Sentinel" } });
    fireEvent.change(roleInput, { target: { value: "Security Analyst" } });

    fireEvent.click(screen.getByRole("button", { name: /Create Teammate/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "create_agent_profile",
        expect.objectContaining({
          request: expect.objectContaining({
            id: "sentinel-reviewer",
            name: "Sentinel",
            role: "Security Analyst",
          }),
        })
      );
    });
  });

  it("deletes a custom profile after confirmation", async () => {
    render(
      <TeammateRosterModal
        isOpen={true}
        onClose={vi.fn()}
        selectedBotId="other"
        onSelectBot={vi.fn()}
      />
    );

    // Expand the profile actions to reach the delete control.
    await waitFor(() => {
      expect(screen.getByText("@Sentinel")).toBeInTheDocument();
    });

    const manage = screen.getAllByTitle("Manage")[0];
    fireEvent.click(manage);

    const deleteBtn = await screen.findByRole("button", { name: /Delete profile/i });
    fireEvent.click(deleteBtn);

    // The confirm dialog carries the same accessible name; it renders after
    // the roster card, so take the last match.
    const confirmBtns = screen.getAllByRole("button", { name: /^Delete profile$/i });
    fireEvent.click(confirmBtns[confirmBtns.length - 1]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_agent_profile", { id: "sentinel-reviewer" });
    });
  });
});

describe("ArtifactsModal Filtering, Comments & Comparison", () => {
  const mockArtifacts = [
    {
      id: "art_img_1",
      conversation_id: "conv_1",
      name: "homepage_preview_1920x1080.png",
      artifact_type: "image",
      file_path: "/data/artifacts/homepage.png",
      mime_type: "image/png",
      size_bytes: 512000,
      metadata_json: JSON.stringify({ comments: [{ timestamp: 1000, comment: "Looks clean" }] }),
      created_at: 1000,
    },
    {
      id: "art_code_2",
      conversation_id: "conv_1",
      name: "server_patch.rs",
      artifact_type: "code",
      file_path: "/data/artifacts/server_patch.rs",
      mime_type: "text/x-rust",
      size_bytes: 4096,
      metadata_json: JSON.stringify({ comments: [] }),
      created_at: 2000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_artifacts") return Promise.resolve(mockArtifacts);
      if (cmd === "read_artifact_content") return Promise.resolve("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");
      if (cmd === "add_artifact_comment") return Promise.resolve(true);
      if (cmd === "delete_artifact") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("filters artifacts by type and searches by name", async () => {
    render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText("homepage_preview_1920x1080.png")[0]).toBeInTheDocument();
    });

    const searchInput = screen.getByPlaceholderText(/Filter artifacts/i);
    fireEvent.change(searchInput, { target: { value: "server_patch" } });

    await waitFor(() => {
      expect(screen.getByText("server_patch.rs")).toBeInTheDocument();
    });
  });

  it("adds comment feedback and triggers artifact deletion", async () => {
    render(<ArtifactsModal isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText("homepage_preview_1920x1080.png")[0]).toBeInTheDocument();
    });

    const commentInput = screen.getByPlaceholderText(/Add comment or feedback/i);
    fireEvent.change(commentInput, { target: { value: "Verify navigation bar responsiveness" } });

    const sendBtn = screen.getByRole("button", { name: /Send Feedback/i });
    fireEvent.click(sendBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "add_artifact_comment",
        expect.objectContaining({
          comment: "Verify navigation bar responsiveness",
          feedToAgent: true,
        })
      );
    });

    const deleteIcon = screen.getByRole("button", { name: /^Delete$/i });
    fireEvent.click(deleteIcon);

    const confirmBtn = screen.getByRole("button", { name: /^Delete Artifact$/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_artifact", { id: "art_img_1" });
    });
  });
});

describe("OnboardingModal Multi-Step Flow", () => {
  const mockSuggested = [
    {
      id: "downloads",
      name: "Downloads",
      path: "/home/snigdha/Downloads",
      exists: true,
      description: "Default download folder",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "get_suggested_folders") return Promise.resolve(mockSuggested);
      if (cmd === "save_api_keys") return Promise.resolve(true);
      if (cmd === "add_authorized_folder") return Promise.resolve({ id: "1", path: "/test" });
      return Promise.resolve(null);
    });
  });

  it("renders onboarding modal and skips to finish", async () => {
    const handleClose = vi.fn();

    render(
      <OnboardingModal
        isOpen={true}
        onClose={handleClose}
        onTryExample={vi.fn()}
        onOpenConnections={vi.fn()}
      />
    );

    expect(screen.getByText("Your Everyday Personal AI Assistant")).toBeInTheDocument();

    const skipBtn = screen.getByTitle("Skip onboarding");
    fireEvent.click(skipBtn);

    expect(handleClose).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem("kyro_onboarding_completed")).toBe("true");
  });
});

describe("TimelineDrawer & TerminalView Subsystems", () => {
  const mockTimelineSteps = [
    {
      step: 1,
      action: "click",
      target: "login_button",
      diff_percent: 4.5,
      before_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      after_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    },
    {
      step: 2,
      action: "type",
      target: "password_input",
      diff_percent: 1.2,
      before_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      after_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "get_timeline_steps") return Promise.resolve(mockTimelineSteps);
      if (cmd === "clear_timeline_steps") return Promise.resolve(true);
      if (cmd === "spawn_terminal") return Promise.resolve(true);
      if (cmd === "write_terminal_input") return Promise.resolve(true);
      if (cmd === "resize_terminal") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("inspects timeline steps, navigates diffs, and clears steps", async () => {
    render(<TimelineDrawer isOpen={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("2 Steps Logged")).toBeInTheDocument();
      expect(screen.getAllByText(/Step 2/i).length).toBeGreaterThan(0);
    });

    const step1Btn = screen.getByRole("button", { name: /Step 1/i });
    fireEvent.click(step1Btn);

    await waitFor(() => {
      expect(screen.getAllByText(/Step 1/i).length).toBeGreaterThan(0);
    });

    const clearBtn = screen.getByRole("button", { name: /^Clear$/i });
    fireEvent.click(clearBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_timeline_steps");
    });
  });

  it("renders TerminalView and initializes PTY", async () => {
    render(<TerminalView />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("spawn_terminal");
    });

    const fitBtn = screen.getByRole("button", { name: /Fit/i });
    fireEvent.click(fitBtn);
  });
});

describe("App Conversation & Active Run Edge Cases", () => {
  const mockConversations = [
    {
      id: "conv_a",
      title: "Task A",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 1000,
      updated_at: 1000,
    },
    {
      id: "conv_b",
      title: "Task B",
      model: "nvidia/nemotron-3-ultra-550b-a55b",
      created_at: 2000,
      updated_at: 2000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.setItem("kyro_onboarding_completed", "true");

    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === "list_conversations") return Promise.resolve(mockConversations);
      if (cmd === "get_messages") return Promise.resolve([]);
      if (cmd === "list_bots") return Promise.resolve([]);
      if (cmd === "get_takeover_status") return Promise.resolve({ is_active: false });
      if (cmd === "list_pending_approvals") return Promise.resolve([]);
      if (cmd === "delete_conversation") return Promise.resolve(true);
      if (cmd === "stop_agent_run") return Promise.resolve(true);
      return Promise.resolve(null);
    });
  });

  it("deletes active session and automatically transitions to next conversation", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task A")).toBeInTheDocument();
      expect(screen.getByText("Task B")).toBeInTheDocument();
    });

    const deleteIcons = screen.getAllByRole("button", { name: /Delete session/i });
    fireEvent.click(deleteIcons[0]);

    const confirmBtn = screen.getByTestId("confirm-delete-session-button");
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_conversation", { id: "conv_a" });
    });
  });

  it("handles agent:step live event streaming and updates active run", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Task A")).toBeInTheDocument();
    });

    if (eventListeners["agent:step"]) {
      eventListeners["agent:step"]({
        payload: {
          run_id: "run_active_100",
          conversation_id: "conv_a",
          step: 1,
          action: "analyze_codebase",
          tool: "ast_grep",
          thought: "Scanning AST for unused variables",
          status: "running",
          tool_calls: [],
          tool_results: [],
          timestamp: 1000,
        },
      });
    }

    await waitFor(() => {
      expect(screen.getByText("Scanning AST for unused variables")).toBeInTheDocument();
    });
  });
});
