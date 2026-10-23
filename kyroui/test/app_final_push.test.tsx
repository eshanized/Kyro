import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../App";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: vi.fn(),
  isPermissionGranted: vi.fn(() => Promise.resolve(true)),
  requestPermission: vi.fn(() => Promise.resolve("granted")),
}));

describe("App.tsx Final Coverage Maximization", () => {
  const mockedInvoke = vi.mocked(invoke);

  beforeEach(() => {
    vi.clearAllMocks();
    window.confirm = vi.fn(() => true);
    localStorage.clear();
    localStorage.setItem("kyro_onboarding_completed", "true");
  });

  /* -------------------------------------------------------------
     1. Everyday Assistant Sub-Views & Delegation Callbacks
     ------------------------------------------------------------- */
  it("navigates everyday tabs and triggers deep delegation callbacks", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return [];
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "list_authorized_folders") return [];
      if (cmd === "list_routines") return [];
      if (cmd === "get_automation_metrics") {
        return {
          total_runs: 5,
          successful_runs: 5,
          failed_runs: 0,
          success_rate: 100,
          total_hours_saved: 2,
          active_routines_count: 1,
        };
      }
      return [];
    });

    render(<App />);

    // 1. Switch to Today tab using sidebar button
    const todayTabBtns = screen.getAllByRole("button", { name: /Today/i });
    fireEvent.click(todayTabBtns[0]);

    await waitFor(() => {
      expect(screen.getByText(/Here is your daily briefing/i)).toBeInTheDocument();
    });

    // Today tab callbacks: Authorize folders navigates to Tools tab
    fireEvent.click(screen.getByText("Authorize Folders"));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "My Tools" })).toBeInTheDocument();
    });

    // 2. Switch back to Today tab
    fireEvent.click(screen.getAllByRole("button", { name: /Today/i })[0]);
    await waitFor(() => {
      expect(screen.getByText(/Here is your daily briefing/i)).toBeInTheDocument();
    });

    // Today tab callback: Create Routine navigates to Routines tab
    fireEvent.click(screen.getByText("Create a Routine"));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "My Routines" })).toBeInTheDocument();
    });

    // 3. Switch back to Ask Kyro tab
    fireEvent.click(screen.getAllByRole("button", { name: /Ask Kyro/i })[0]);
    await waitFor(() => {
      expect(screen.getByText("Kyro Everyday Assistant")).toBeInTheDocument();
    });

    // Everyday empty state shortcuts:
    // Screen Explainer shortcut card
    fireEvent.click(screen.getAllByText("Explain Screen")[1]);
    await waitFor(() => {
      expect(screen.getByText("Explain My Screen")).toBeInTheDocument();
    });
    const xBtn1 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn1) fireEvent.click(xBtn1);

    // File Organizer shortcut card
    fireEvent.click(screen.getAllByText("Organize Files")[1]);
    await waitFor(() => {
      expect(screen.getByText("Safe File Organizer")).toBeInTheDocument();
    });
    const xBtn2 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn2) fireEvent.click(xBtn2);

    // Document Assistant shortcut card
    fireEvent.click(screen.getAllByText("Document Assistant")[1]);
    await waitFor(() => {
      expect(screen.getByText("Local Document Assistant")).toBeInTheDocument();
    });
    const xBtn3 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn3) fireEvent.click(xBtn3);

    // Routines shortcut card
    fireEvent.click(screen.getByText("Automate Routine"));
    await waitFor(() => {
      expect(screen.getByText("Teach-a-Task & Routine Scheduler")).toBeInTheDocument();
    });
    const xBtn4 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn4) fireEvent.click(xBtn4);
  });

  /* -------------------------------------------------------------
     2. Sidebar Quick Modals Opening and Closing
     ------------------------------------------------------------- */
  it("opens and closes all sidebar modals directly", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return [];
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return [];
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

    // Open Screen Explainer from sidebar button
    fireEvent.click(screen.getAllByText("Explain Screen")[0]);
    await waitFor(() => {
      expect(screen.getByText("Explain My Screen")).toBeInTheDocument();
    });
    const xBtn1 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn1) fireEvent.click(xBtn1);

    // Open File Organizer from sidebar button
    fireEvent.click(screen.getAllByText("Organize Files")[0]);
    await waitFor(() => {
      expect(screen.getByText("Safe File Organizer")).toBeInTheDocument();
    });
    const xBtn2 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn2) fireEvent.click(xBtn2);

    // Open Document Assistant from sidebar button
    fireEvent.click(screen.getAllByText("Document Assistant")[0]);
    await waitFor(() => {
      expect(screen.getByText("Local Document Assistant")).toBeInTheDocument();
    });
    const xBtn3 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn3) fireEvent.click(xBtn3);

    // Open Routines from sidebar button
    fireEvent.click(screen.getByText("Automations & Routines"));
    await waitFor(() => {
      expect(screen.getByText("Teach-a-Task & Routine Scheduler")).toBeInTheDocument();
    });
    const xBtn4 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn4) fireEvent.click(xBtn4);

    // Open Artifacts from sidebar button
    fireEvent.click(screen.getByText("Artifacts Registry"));
    await waitFor(() => {
      expect(screen.getByText(/Local Artifact Registry/i)).toBeInTheDocument();
    });
    const xBtn5 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn5) fireEvent.click(xBtn5);

    // Open Connections from sidebar button
    fireEvent.click(screen.getByText("App Connections"));
    await waitFor(() => {
      expect(screen.getByText("Connections & App Integrations")).toBeInTheDocument();
    });
    const xBtn6 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn6) fireEvent.click(xBtn6);
  });

  /* -------------------------------------------------------------
     3. Command Palette Full Execution Suite
     ------------------------------------------------------------- */
  it("executes all Command Palette actions and keyboard shortcut", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") return [];
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "create_conversation") {
        return { id: "conv-palette", title: "New Session", model: "llama" };
      }
      if (cmd === "toggle_ghost_display") return { active: true };
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

    const openPalette = () => {
      fireEvent.click(screen.getByTitle("Quick Actions & Search (Ctrl+K)"));
    };

    // Open via Ctrl+K
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    if (!screen.queryByPlaceholderText(/Type a command or search/i)) {
      openPalette();
    }
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Type a command or search/i)).toBeInTheDocument();
    });

    // 1. New Autonomous Session
    fireEvent.click(screen.getByText("New Autonomous Session"));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("create_conversation", expect.any(Object));
    });

    // 2. Toggle Ghost Mode
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Toggle Ghost Mode")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Toggle Ghost Mode"));

    // 3. Toggle Terminal
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Toggle PTY Host Terminal")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Toggle PTY Host Terminal"));
    await waitFor(() => {
      expect(screen.getByText(/Host Terminal PTY/i)).toBeInTheDocument();
    });
    // Close terminal
    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    // 4. Open Artifacts Registry
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Open Artifacts Registry")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Open Artifacts Registry"));
    await waitFor(() => {
      expect(screen.getByText(/Local Artifact Registry/i)).toBeInTheDocument();
    });
    const xBtn1 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn1) fireEvent.click(xBtn1);

    // 5. Teach-a-Task & Routines
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Teach-a-Task & Routines")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Teach-a-Task & Routines"));
    await waitFor(() => {
      expect(screen.getByText("Teach-a-Task & Routine Scheduler")).toBeInTheDocument();
    });
    const xBtn2 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn2) fireEvent.click(xBtn2);

    // 6. Teammate Roster
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Teammate Roster")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Teammate Roster"));
    await waitFor(() => {
      expect(screen.getByText(/Teammate Roster & Agent Profiles/i)).toBeInTheDocument();
    });
    const xBtn3 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn3) fireEvent.click(xBtn3);

    // 7. App Connections
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("App Connections & Integrations")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("App Connections & Integrations"));
    await waitFor(() => {
      expect(screen.getByText("Connections & App Integrations")).toBeInTheDocument();
    });
    const xBtn4 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn4) fireEvent.click(xBtn4);

    // 8. Execution Timeline
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Step Timeline & Visual Diffs")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Step Timeline & Visual Diffs"));
    await waitFor(() => {
      expect(screen.getByText(/Visual Step Timeline & Diff Inspector/i)).toBeInTheDocument();
    });
    const xBtn5 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn5) fireEvent.click(xBtn5);

    // 9. Settings
    openPalette();
    await waitFor(() => {
      expect(screen.getByText("Settings & Vault")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Settings & Vault"));
    await waitFor(() => {
      expect(screen.getByText(/Kyro System Settings & Security Matrix/i)).toBeInTheDocument();
    });
    const xBtn6 = document.querySelector(".lucide-x")?.closest("button");
    if (xBtn6) fireEvent.click(xBtn6);
  });

  /* -------------------------------------------------------------
     4. File Attachments: Text file and Image File Submissions
     ------------------------------------------------------------- */
  it("attaches text file and submits prompt with attached context", async () => {
    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") {
        return [{ id: "conv-main", title: "Main Chat", model: "llama", created_at: 1000 }];
      }
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return [];
      if (cmd === "add_message") return {};
      if (cmd === "run_agent_turn") return "Analyzed attached file successfully.";
      return [];
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Ask @/i)).toBeInTheDocument();
    });

    // Mock FileReader
    class MockFileReader {
      result: string = "function test() { return 42; }";
      onload: any = null;
      readAsText() {
        if (this.onload) this.onload();
      }
      readAsDataURL() {
        this.result = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
        if (this.onload) this.onload();
      }
    }
    (window as any).FileReader = MockFileReader;

    // Attach text file
    const file = new File(["function test() { return 42; }"], "test.js", { type: "text/javascript" });
    const fileInput = document.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(fileInput, { target: { files: [file] } });

    await waitFor(() => {
      expect(screen.getByText("test.js")).toBeInTheDocument();
    });

    // Submit form with prompt
    const promptInput = screen.getByPlaceholderText(/Ask @/i);
    fireEvent.change(promptInput, { target: { value: "Review this code" } });
    fireEvent.submit(promptInput.closest("form")!);

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("run_agent_turn", expect.objectContaining({
        history: expect.arrayContaining([
          expect.objectContaining({
            content: expect.stringContaining("[Attachment: test.js]"),
          }),
        ]),
      }));
    });
  });

  /* -------------------------------------------------------------
     5. Approvals: Denying Pending Approval Request
     ------------------------------------------------------------- */
  it("denies pending approval request safely", async () => {
    const mockApprovals = [
      {
        id: "appr-deny-1",
        run_id: "run-101",
        conversation_id: "conv-main",
        tool_name: "rm_rf",
        summary: "rm -rf /tmp/data",
        risk_level: "destructive",
        status: "pending",
        requested_at: Date.now(),
      },
    ];

    mockedInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "list_conversations") {
        return [{ id: "conv-main", title: "Main Chat", model: "llama", created_at: 1000 }];
      }
      if (cmd === "get_messages") return [];
      if (cmd === "list_personas") return [];
      if (cmd === "get_takeover_status") return { active: false };
      if (cmd === "list_pending_approvals") return mockApprovals;
      if (cmd === "resolve_approval") return {};
      return [];
    });

    render(<App />);

    await waitFor(
      () => {
        expect(screen.getByText("Tool Approval Required:")).toBeInTheDocument();
        expect(screen.getByText("rm -rf /tmp/data")).toBeInTheDocument();
      },
      { timeout: 4000 }
    );

    // Click Deny
    fireEvent.click(screen.getByText("Deny"));
    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("resolve_approval", {
        id: "appr-deny-1",
        decision: "denied",
        resolvedBy: "desktop_user",
      });
    });
  });
});
