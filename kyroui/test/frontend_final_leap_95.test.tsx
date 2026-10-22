import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { ConnectionsModal } from "../components/ConnectionsModal";
import { TeachTaskModal } from "../components/TeachTaskModal";
import { MyRoutinesView } from "../components/MyRoutinesView";
import { SettingsModal } from "../components/SettingsModal";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe("Frontend Final Leap to >= 95% Coverage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    localStorage.setItem("kyro_onboarding_completed", "true");
  });

  /* --------------------------------------------------------------------------
     1. App.tsx Dev Mode Features, Top Nav, Screen Inspection & Abort
     -------------------------------------------------------------------------- */
  it("exercises dev mode switcher, top nav actions, grounded screen, and abort", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") {
        return [{ id: "conv-dev-1", title: "Dev Session", model: "nemotron", created_at: 1000 }];
      }
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { is_active: true, reason: "Manual intervention required" };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "resume_agent_execution") return {};
      if (cmd === "take_screenshot") return { base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==" };
      if (cmd === "capture_grounded_screen") {
        return {
          base64_png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
          source: "CDP",
          marks: [
            { id: 1, label: "Submit Button", bbox: [100, 200, 150, 250], confidence: 0.98 },
          ],
        };
      }
      if (cmd === "toggle_ghost_mode") return { enabled: true };
      if (cmd === "cancel_agent_run") return {};
      if (cmd === "abort_computer_use") return {};
      if (cmd === "toggle_hud_window") return {};
      if (cmd === "list_artifacts") return [];
      if (cmd === "list_skills") return [];
      if (cmd === "list_routines") return [];
      if (cmd === "list_connector_catalog") return [];
      if (cmd === "list_connected_accounts") return [];
      if (cmd === "list_mcp_servers") return [];
      if (cmd === "get_mcp_server_statuses") return {};
      return [];
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("KYRO")).toBeInTheDocument();
    });

    // 1. Resume Takeover from banner
    await waitFor(
      () => {
        expect(screen.getByText(/Human Takeover Active/i)).toBeInTheDocument();
      },
      { timeout: 3000 }
    );
    fireEvent.click(screen.getByText("Resume Autonomous Agent"));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("resume_takeover");
    });

    // 2. Toggle HUD window from header
    fireEvent.click(screen.getByTitle("Toggle Floating HUD Widget"));
    expect(mockedInvoke).toHaveBeenCalledWith("toggle_hud_window");

    // 3. Switch experience mode to Dev Mode
    fireEvent.click(screen.getByRole("button", { name: /Dev Mode/i }));
    await waitFor(() => {
      expect(screen.getByTitle(/Open Developer Workbench/i)).toBeInTheDocument();
    });

    // 4. In Dev Mode, click Ghost Display toggle
    fireEvent.click(screen.getByRole("button", { name: /Ghost/i }));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("toggle_ghost_mode", { enabled: true });
    });

    // 5. In Dev Mode, click Inspect Elements (Grounded Screen)
    fireEvent.click(screen.getByRole("button", { name: /Ground/i }));
    await waitFor(() => {
      expect(screen.getByText(/Set-of-Marks \(SoM\) Grounding Active/i)).toBeInTheDocument();
    });
    // Close Grounded modal
    const closeBtn = document.querySelector(".lucide-x")?.closest("button");
    if (closeBtn) fireEvent.click(closeBtn);

    // 6. Sidebar Dev Mode buttons
    fireEvent.click(screen.getByText("Dev Workbench"));
    await waitFor(() => {
      expect(screen.getByText(/Kyro Developer Workbench/i)).toBeInTheDocument();
    });
    const closeWb = document.querySelector(".lucide-x")?.closest("button");
    if (closeWb) fireEvent.click(closeWb);

    // MCP Tool Servers from sidebar
    fireEvent.click(screen.getByText("MCP Tool Servers"));
    await waitFor(() => {
      expect(screen.getByText("Model Context Protocol (MCP) Tools")).toBeInTheDocument();
    });
    const closeMcp = document.querySelector(".lucide-x")?.closest("button");
    if (closeMcp) fireEvent.click(closeMcp);

    // Teammate Roster from sidebar
    fireEvent.click(screen.getByText("Teammate Roster"));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: /Teammate Roster/i })).toBeInTheDocument();
    });
    const closeTeam = document.querySelector(".lucide-x")?.closest("button");
    if (closeTeam) fireEvent.click(closeTeam);

    // 7. Switch back to Everyday Mode
    fireEvent.click(screen.getByRole("button", { name: /Everyday/i }));
    await waitFor(() => {
      expect(screen.getByTitle("Capture display snapshot")).toBeInTheDocument();
    });

    // 8. Capture display snapshot (Screen)
    fireEvent.click(screen.getByTitle("Capture display snapshot"));
    await waitFor(() => {
      expect(screen.getByText("Display Snapshot ready")).toBeInTheDocument();
    });
    const dismissButtons = screen.getAllByText("Dismiss");
    dismissButtons.forEach((btn) => fireEvent.click(btn));
  });

  /* --------------------------------------------------------------------------
     2. Voice Dictation, Text-to-Speech & Speech API Fallbacks
     -------------------------------------------------------------------------- */
  it("tests voice dictation fallback and text-to-speech audio synthesis", async () => {
    mockedInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_conversations") return [{ id: "c1", title: "Audio Test", model: "nemotron" }];
      if (cmd === "get_messages") {
        return [
          { id: "m1", conversation_id: "c1", role: "assistant", content: "Hello, I am Kyro.", created_at: 100 },
        ];
      }
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { is_active: false };
      if (cmd === "list_pending_approvals") return [];
      return [];
    });

    // Mock speechSynthesis
    const speakMock = vi.fn();
    const cancelMock = vi.fn();
    window.speechSynthesis = {
      speak: speakMock,
      cancel: cancelMock,
    } as any;
    (window as any).SpeechSynthesisUtterance = class {
      text: string;
      constructor(t: string) {
        this.text = t;
      }
    };

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("Hello, I am Kyro.")).toBeInTheDocument();
    });

    // Click speech synthesis button for assistant message
    const listenBtn = screen.getByTitle("Read message aloud (Local TTS)");
    fireEvent.click(listenBtn);
    expect(speakMock).toHaveBeenCalled();

    // Click again to toggle cancel
    fireEvent.click(listenBtn);
    expect(cancelMock).toHaveBeenCalled();

    // Voice dictation without SpeechRecognition
    const micBtn = screen.getByTitle("Voice Dictation (Speech to Text)");
    fireEvent.click(micBtn);
    await waitFor(() => {
      expect(screen.getByText(/Speech recognition is not supported/i)).toBeInTheDocument();
    });
  });

  /* --------------------------------------------------------------------------
     3. Image Upload and Consequential Execution Plan
     -------------------------------------------------------------------------- */
  it("uploads image file and displays structured consequential plan", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return [{ id: "conv-img", title: "Img Chat", model: "nemotron" }];
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") return "Organized 12 files successfully into categories.";
      return [];
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
    });

    // Mock FileReader with readAsDataURL
    class MockFileReaderDataUrl {
      onload: (() => void) | null = null;
      result: string = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
      readAsDataURL(file: any) {
        setTimeout(() => {
          if (this.onload) this.onload();
        }, 10);
      }
      readAsText(file: any) {
        this.result = "text content";
        setTimeout(() => {
          if (this.onload) this.onload();
        }, 10);
      }
    }
    vi.stubGlobal("FileReader", MockFileReaderDataUrl);

    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    const testImage = new File(["dummy png"], "chart.png", { type: "image/png" });

    fireEvent.change(fileInput, { target: { files: [testImage] } });

    await waitFor(() => {
      expect(screen.getByText("chart.png")).toBeInTheDocument();
    });

    // Enter prompt with consequential keyword "organize"
    const input = screen.getByPlaceholderText(/Ask @/i);
    fireEvent.change(input, { target: { value: "Please organize and clean my documents" } });
    fireEvent.submit(input.closest("form")!);

    // Should render Execution Plan
    await waitFor(() => {
      expect(screen.getByText("Execution Plan")).toBeInTheDocument();
      expect(screen.getByText("Organized 12 files successfully into categories.")).toBeInTheDocument();
    });

    vi.unstubAllGlobals();
  });

  /* --------------------------------------------------------------------------
     4. ConnectionsModal Local MCP & Dialog Deep Dive
     -------------------------------------------------------------------------- */
  it("tests ConnectionsModal local MCP creation, deletion, and connect dialogs", async () => {
    const mockCatalog = [
      {
        id: "slack",
        name: "Slack",
        connector_type: "native" as const,
        description: "Post messages and read channels",
        category: "communication",
        auth_type: "token",
        scopes: ["chat:write"],
        required_scopes: ["chat:write"],
        capabilities: ["messages"],
        is_builtin: true,
        is_available: true,
        connected_accounts_count: 0,
        status: "available" as const,
        icon: "slack",
      },
    ];

    const mockMcpServers = [
      {
        id: "srv-local-1",
        name: "Local FS",
        command: "npx",
        args_json: '["-y", "mcp-server"]',
        enabled: true,
        created_at: 1000,
      },
    ];

    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_connector_catalog") return mockCatalog;
      if (cmd === "list_connected_accounts") return [];
      if (cmd === "list_mcp_servers") return mockMcpServers;
      if (cmd === "get_mcp_server_statuses") return { "srv-local-1": "connected" };
      if (cmd === "create_mcp_server") return { id: "srv-new-2" };
      if (cmd === "delete_mcp_server") return {};
      if (cmd === "connect_account_token") return { id: "acc-new" };
      return [];
    });

    render(<ConnectionsModal isOpen={true} onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Connections & App Integrations")).toBeInTheDocument();
    });

    // 1. Switch to Local MCP Servers tab
    fireEvent.click(screen.getByRole("button", { name: /Local MCP Servers/i }));
    await waitFor(() => {
      expect(screen.getByText("Configured Local Servers (1)")).toBeInTheDocument();
      expect(screen.getByText("Local FS")).toBeInTheDocument();
    });

    // Click "Add Local MCP Server"
    fireEvent.click(screen.getByText("Add Local MCP Server"));
    expect(screen.getByText("New Local MCP Configuration")).toBeInTheDocument();

    // Fill form
    fireEvent.change(screen.getByPlaceholderText("e.g. Local Filesystem Server"), {
      target: { value: "My Dev Server" },
    });
    fireEvent.change(screen.getByPlaceholderText("npx or python3"), {
      target: { value: "node" },
    });
    fireEvent.change(screen.getByPlaceholderText(/-y @modelcontextprotocol/), {
      target: { value: "index.js --port 8080" },
    });

    // Add env variable with auto secret detection
    fireEvent.change(screen.getByPlaceholderText(/KEY \(e\.g\. DATABASE_URL/), {
      target: { value: "MY_SECRET_TOKEN" },
    });
    fireEvent.change(screen.getByPlaceholderText(/VALUE/), {
      target: { value: "supersecret123" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add Env" }));

    // Click Save & Register
    fireEvent.click(screen.getByText("Save & Register Local Server"));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("create_mcp_server", {
        server: expect.objectContaining({
          name: "My Dev Server",
          command: "node",
        }),
      });
    });

    // Delete local MCP server
    fireEvent.click(screen.getByTitle("Delete server"));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("delete_mcp_server", { id: "srv-local-1" });
    });

    // 2. Switch back to Discover Catalog tab and filter by Native API
    fireEvent.click(screen.getByRole("button", { name: /Discover Catalog/i }));
    await waitFor(() => {
      expect(screen.getByText("Slack")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Native API" }));
    expect(screen.getByText("Slack")).toBeInTheDocument();

    // Click Connect on Slack
    fireEvent.click(screen.getByRole("button", { name: "Connect Account" }));
    await waitFor(() => {
      expect(screen.getByText("Connect Slack")).toBeInTheDocument();
    });

    // Enter name & token in connect dialog and save
    fireEvent.change(screen.getByPlaceholderText("e.g. Work GitHub or Personal"), {
      target: { value: "Work Slack" },
    });
    fireEvent.change(screen.getByPlaceholderText("Secret bearer token"), {
      target: { value: "xoxb-test-token" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save & Connect" }));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("connect_account_token", expect.objectContaining({
        connectorId: "slack",
        displayName: "Work Slack",
      }));
    });
  });

  /* --------------------------------------------------------------------------
     5. TeachTaskModal Routine Selection, Drawer Details and Deletion
     -------------------------------------------------------------------------- */
  it("selects routine in TeachTaskModal, views execution drawer, and confirms delete", async () => {
    let currentRoutines = [
      {
        id: "rt-inspect-1",
        name: "Daily Backup Routine",
        cron_expression: "0 0 * * *",
        action_type: "command",
        target_id: "tar -czf /tmp/backup.tar.gz /data",
        enabled: true,
        last_run: 1700000000000,
        next_run: 1700086400000,
      },
    ];

    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_routines") return currentRoutines;
      if (cmd === "list_skills") return [];
      if (cmd === "delete_routine") {
        currentRoutines = [];
        return true;
      }
      return [];
    });

    render(<TeachTaskModal isOpen={true} onClose={() => {}} initialTab="routines" />);

    await waitFor(() => {
      expect(screen.getByText("Daily Backup Routine")).toBeInTheDocument();
    });

    // Click routine to open Details Drawer
    fireEvent.click(screen.getByText("Daily Backup Routine"));
    await waitFor(() => {
      expect(screen.getByText("Last Executed")).toBeInTheDocument();
      expect(screen.getByText("Next Scheduled")).toBeInTheDocument();
    });

    // Click Delete task in drawer
    const deleteBtn = screen.getAllByRole("button", { name: "Delete task" })[0];
    fireEvent.click(deleteBtn);
    await waitFor(() => {
      expect(screen.getByText("Delete task?")).toBeInTheDocument();
    });

    // Confirm deletion in dialog
    const dialog = screen.getByRole("dialog");
    const confirmBtn = within(dialog).getByRole("button", { name: "Delete task" });
    fireEvent.click(confirmBtn);
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("delete_routine", { id: "rt-inspect-1" });
    });
  });

  /* --------------------------------------------------------------------------
     6. MyRoutinesView Edit Routine Modal & History Switching
     -------------------------------------------------------------------------- */
  it("edits a routine and navigates execution history in MyRoutinesView", async () => {
    const mockRoutines = [
      {
        id: "rt-edit-1",
        name: "Morning Report",
        cron_expression: "0 8 * * 1-5",
        action_type: "skill",
        target_id: "Generate Morning Briefing",
        enabled: true,
        last_run: Date.now() - 3600000,
        next_run: Date.now() + 86400000,
      },
    ];

    const mockHistory = [
      {
        id: "run-h-1",
        routine_id: "rt-edit-1",
        routine_name: "Morning Report",
        started_at: Date.now() - 3600000,
        finished_at: Date.now() - 3500000,
        status: "succeeded",
        duration_ms: 100000,
        summary: "Generated 3 page summary report",
      },
    ];

    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_routines") return mockRoutines;
      if (cmd === "list_routine_runs") return mockHistory;
      if (cmd === "update_routine") return {};
      if (cmd === "execute_routine_now") return { status: "succeeded", output_summary: "Done" };
      return [];
    });

    const onAskPrompt = vi.fn();
    render(<MyRoutinesView onAskPrompt={onAskPrompt} />);

    await waitFor(() => {
      expect(screen.getByText("Morning Report")).toBeInTheDocument();
    });

    // 1. Run routine now
    fireEvent.click(screen.getByRole("button", { name: "Run Now" }));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("execute_routine_now", { routineId: "rt-edit-1" });
    });

    // 2. Open Edit Routine Modal
    fireEvent.click(screen.getByTitle("Edit routine"));
    await waitFor(() => {
      expect(screen.getByText("Edit Routine")).toBeInTheDocument();
    });

    // Edit fields
    const nameInput = screen.getByDisplayValue("Morning Report");
    fireEvent.change(nameInput, { target: { value: "Morning Executive Report" } });

    const cronInput = screen.getByDisplayValue("0 8 * * 1-5");
    fireEvent.change(cronInput, { target: { value: "0 9 * * 1-5" } });

    // Save changes
    fireEvent.click(screen.getByRole("button", { name: "Save Changes" }));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("update_routine", {
        routine: expect.objectContaining({
          id: "rt-edit-1",
          name: "Morning Executive Report",
          cron_expression: "0 9 * * 1-5",
        }),
      });
    });

    // 3. Switch to Run History tab
    fireEvent.click(screen.getByRole("button", { name: /Run History/i }));
    await waitFor(() => {
      expect(screen.getByText("Generated 3 page summary report")).toBeInTheDocument();
    });
  });

  /* --------------------------------------------------------------------------
     7. SettingsModal Tab Navigation, Diagnostics & Vault
     -------------------------------------------------------------------------- */
  it("navigates all tabs in SettingsModal including diagnostics and vault", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_settings") {
        return {
          default_model: "llama-3",
          auto_approve_safe_tools: true,
          xvfb_enabled: true,
          require_terminal_approval: true,
          max_daily_budget_usd: 10,
        };
      }
      if (cmd === "get_api_keys") return { has_nvidia: true };
      if (cmd === "get_governance_policy") {
        return {
          shell_mode: "SafeAutoApprove",
          computer_use_mode: "FullAutonomous",
          require_takeover_for_credentials: true,
          blocked_domains: ["chase.com", "paypal.com"],
        };
      }
      if (cmd === "get_sandbox_status") return { enabled: true, engine: "docker", isolated: true };
      if (cmd === "get_browser_capabilities") return { cdp_available: true, xvfb_active: true };
      if (cmd === "list_authorized_folders") return [];
      if (cmd === "get_system_health") {
        return {
          db_connected: true,
          display_server: "X11 (:0)",
          docker_available: true,
          xvfb_available: true,
        };
      }
      if (cmd === "get_starter_templates") return [];
      if (cmd === "list_vault_keys") {
        return [{ key: "OPENAI_API_KEY", created_at: 100, masked_value: "sk-...1234" }];
      }
      if (cmd === "save_settings") return {};
      if (cmd === "delete_vault_key") return {};
      return [];
    });

    render(<SettingsModal isOpen={true} onClose={() => {}} initialTab="diagnostics" />);

    await waitFor(() => {
      expect(screen.getByText(/Kyro System Settings/i)).toBeInTheDocument();
    });

    // 1. Diagnostics Tab
    await waitFor(() => {
      expect(screen.getByText("Real-Time Local Runtime Diagnostics")).toBeInTheDocument();
      expect(screen.getByText("SQLite Database (WAL Mode)")).toBeInTheDocument();
      expect(screen.getByText("Connected & WAL Active")).toBeInTheDocument();
    });

    // 2. Switch to Model & Credentials tab
    fireEvent.click(screen.getByRole("button", { name: /Model & Credentials/i }));
    await waitFor(() => {
      expect(screen.getByText("Autonomous Model Selection")).toBeInTheDocument();
    });

    // 3. Switch to Governance tab
    fireEvent.click(screen.getByRole("button", { name: /Governance/i }));
    await waitFor(() => {
      expect(screen.getByText(/Human Takeover for Sensitive Checkpoints/i)).toBeInTheDocument();
    });

    // 4. Switch to Privacy tab
    fireEvent.click(screen.getByRole("button", { name: /Privacy & Folders/i }));
    await waitFor(() => {
      expect(screen.getByText("Authorized Folders")).toBeInTheDocument();
    });
  });
});
